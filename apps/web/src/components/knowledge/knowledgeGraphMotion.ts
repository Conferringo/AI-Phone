import type {Model} from './knowledgeGraphLayout';
export type Motion={resize(w:number,h:number,ratio:number):void;highlight(id:string|null):void;target(x:number,y:number):void;move(id:string,dx:number,dy:number):void;panBy(dx:number,dy:number):void;zoomBy(factor:number):void;home():void;sync():void;dispose():void};
type Clock={requestFrame:typeof requestAnimationFrame;cancelFrame:typeof cancelAnimationFrame;now:()=>number;hidden:()=>boolean;reduced:()=>boolean};
type Position={x:number;y:number;offsetX:number;offsetY:number;goalX:number;goalY:number};
type LabelPoint={id:string;x:number;y:number;width:number;height:number};
type LabelBox={x:number;y:number;width:number;height:number};

/** Labels keep their readable pixel size when the graph camera scales down. */
export function placeKnowledgeLabels(points:LabelPoint[],width:number,height:number):Map<string,LabelBox>{
 const placed=new Map<string,LabelBox>(),boxes:LabelBox[]=[];
 const overlap=(a:LabelBox,b:LabelBox)=>Math.max(0,Math.min(a.x+a.width,b.x+b.width)-Math.max(a.x,b.x))*Math.max(0,Math.min(a.y+a.height,b.y+b.height)-Math.max(a.y,b.y));
 for(const point of [...points].sort((a,b)=>a.y-b.y||a.x-b.x||a.id.localeCompare(b.id))){
  const {x,y,width:w,height:h}=point;
  if(x<0||y<0||x>width||y>height){placed.set(point.id,{x:x-w/2,y:y+20,width:w,height:h});continue;}
  let best:LabelBox={x:x-w/2,y:y+20,width:w,height:h},cost=Infinity;
  const consider=(left:number,top:number)=>{
   const box={x:Math.max(6,Math.min(width-w-6,left)),y:Math.max(6,Math.min(height-h-6,top)),width:w,height:h};
   const padded={x:box.x-4,y:box.y-4,width:w+8,height:h+8};
   const collision=boxes.reduce((sum,other)=>sum+overlap(padded,other),0)+points.reduce((sum,p)=>sum+overlap(padded,{x:p.x-9,y:p.y-9,width:18,height:18}),0);
   const distance=Math.hypot(box.x+w/2-x,box.y+h/2-y);
   const score=collision*1000+distance;
   if(score<cost){cost=score;best=box;}
  };
  for(const gap of [20,40,64,90])for(const [left,top] of [[x-w/2,y+gap],[x-w/2,y-h-gap],[x+gap,y-h/2],[x-w-gap,y-h/2],[x+gap,y+gap],[x-w-gap,y+gap],[x+gap,y-h-gap],[x-w-gap,y-h-gap]])consider(left,top);
  if(cost>=1000)for(let top=6;top<=height-h-6;top+=10)for(let left=6;left<=width-w-6;left+=10)consider(left,top);
  placed.set(point.id,best);boxes.push(best);
 }
 return placed;
}

/** Original renderer: time-based easing, bounded camera and per-instance lifecycle. */
export function createKnowledgeGraphMotion({surface,elements,model}:{surface:HTMLCanvasElement;elements:Map<string,HTMLElement>;model:Model},clock:Clock):Motion{
 const {requestFrame,cancelFrame,now,hidden,reduced}=clock,context=surface.getContext('2d');
 const nodes=new Map<string,Position>(model.nodes.map(node=>[node.id,{x:node.x,y:node.y,offsetX:0,offsetY:0,goalX:0,goalY:0}]));
 const radius=model.nodes.reduce((max,node)=>Math.max(max,Math.abs(node.x),Math.abs(node.y)),150);
 const labels=new Map<string,{element:HTMLElement;width:number;height:number}>([...elements].flatMap(([id,element])=>{const label=element.querySelector?.<HTMLElement>('.knowledge-node-label');return label?[[id,{element:label,width:140,height:37}] as [string,{element:HTMLElement;width:number;height:number}]]:[];}));
 let labelLayoutDirty=true;
 let w=0,h=0,dpr=1,magnification=1,dx=0,dy=0,selected:string|null=null,frame=0,closed=false,pulse=0,previous=0;
 const scale=()=>Math.max(.2,Math.min(1.1,w/(radius*2+150),h/(radius*2+110)))*magnification;
 const screen=(node:Position)=>({x:w*.5+dx+(node.x+node.offsetX)*scale(),y:h*.5+dy+(node.y+node.offsetY)*scale()});
 function schedule(){if(!closed&&!frame&&w>0&&h>0&&!hidden())frame=requestFrame(render);}
 function render(){
  frame=0;if(closed||hidden()||!w||!h)return;
  const time=now(),dt=previous?Math.min(64,Math.max(1,time-previous)):16;previous=time;
  const ease=reduced()?1:1-Math.exp(-dt/70);let unsettled=false;
  if(labelLayoutDirty){
   const points=[...nodes].flatMap(([id,node])=>{const label=labels.get(id);return label?[{id,...screen({...node,offsetX:0,offsetY:0}),width:label.width,height:label.height}]:[];});
   const boxes=placeKnowledgeLabels(points,w,h);
   for(const point of points){const box=boxes.get(point.id)!,label=labels.get(point.id)!.element;label.style.left=`${box.x-point.x}px`;label.style.top=`${box.y-point.y}px`;label.style.translate='none';}
   labelLayoutDirty=false;
  }
  for(const [id,node] of nodes){
   node.offsetX+=(node.goalX-node.offsetX)*ease;node.offsetY+=(node.goalY-node.offsetY)*ease;
   if(Math.abs(node.goalX-node.offsetX)+Math.abs(node.goalY-node.offsetY)>.04)unsettled=true;
   else {node.offsetX=node.goalX;node.offsetY=node.goalY;}
   const point=screen(node),element=elements.get(id);
   if(element)element.style.transform=`translate(${point.x}px, ${point.y}px)`;
  }
  const elapsed=time-pulse,animate=!!selected&&!reduced()&&elapsed>=0&&elapsed<950;
  if(context){
   context.setTransform(dpr,0,0,dpr,0,0);context.clearRect(0,0,w,h);
   for(const edge of model.links){
    const start=nodes.get(edge.source_id),end=nodes.get(edge.target_id);if(!start||!end)continue;
    const from=screen(start),to=screen(end),emphasis=edge.source_id===selected||edge.target_id===selected;
    const bend=Math.min(28,Math.hypot(to.x-from.x,to.y-from.y)*.1);
    const c1={x:from.x+(to.x-from.x)*.35,y:from.y+(to.y-from.y)*.35-bend};
    const c2={x:from.x+(to.x-from.x)*.65,y:from.y+(to.y-from.y)*.65-bend};
    context.beginPath();context.moveTo(from.x,from.y);context.bezierCurveTo(c1.x,c1.y,c2.x,c2.y,to.x,to.y);
    context.lineWidth=emphasis?1.25:.8;context.strokeStyle=emphasis?'#303b4466':selected?'#65717c16':'#65717c32';context.stroke();
    if(animate&&emphasis){
     const t=edge.source_id===selected?elapsed/950:1-elapsed/950,u=1-t;
     const x=u*u*u*from.x+3*u*u*t*c1.x+3*u*t*t*c2.x+t*t*t*to.x;
     const y=u*u*u*from.y+3*u*u*t*c1.y+3*u*t*t*c2.y+t*t*t*to.y;
     context.beginPath();context.fillStyle=`rgba(48,59,68,${Math.sin(elapsed/950*Math.PI)*.6})`;context.arc(x,y,2,0,Math.PI*2);context.fill();
    }
   }
  }
  if(unsettled||animate)schedule();else previous=0;
 }
 const resetOffsets=()=>{for(const node of nodes.values())node.offsetX=node.offsetY=node.goalX=node.goalY=0;};
 return {
  resize(width,height,ratio){w=Math.max(0,width);h=Math.max(0,height);labelLayoutDirty=true;dpr=Math.min(2,Math.max(.5,ratio||1));surface.width=Math.round(w*dpr);surface.height=Math.round(h*dpr);for(const label of labels.values()){label.width=label.element.offsetWidth||140;label.height=label.element.offsetHeight||37;}if(!w||!h){cancelFrame(frame);frame=0;}else schedule();},
  highlight(id){if(selected===id)return;selected=id;pulse=now();schedule();},
  target(x,y){
   for(const node of nodes.values()){
    const point=screen({...node,offsetX:0,offsetY:0}),distance=Math.hypot(x-point.x,y-point.y);
    const amount=Number.isFinite(distance)&&distance<100&&!reduced()?(1-distance/100)*.09/scale():0;
    node.goalX=amount?(x-point.x)*amount:0;node.goalY=amount?(y-point.y)*amount:0;
   }
   schedule();
  },
  move(id,x,y){const node=nodes.get(id);if(!node||!Number.isFinite(x+y))return;node.x+=x/scale();node.y+=y/scale();labelLayoutDirty=true;schedule();},
  panBy(x,y){if(!Number.isFinite(x+y))return;dx+=x;dy+=y;labelLayoutDirty=true;schedule();},
  zoomBy(factor){if(!Number.isFinite(factor)||factor<=0)return;magnification=Math.min(3,Math.max(.45,magnification*factor));labelLayoutDirty=true;schedule();},
  home(){magnification=1;dx=dy=0;resetOffsets();labelLayoutDirty=true;schedule();},
  sync(){cancelFrame(frame);frame=0;previous=0;if(reduced())resetOffsets();schedule();},
  dispose(){if(closed)return;closed=true;cancelFrame(frame);frame=0;context?.clearRect(0,0,surface.width,surface.height);nodes.clear();},
 };
}
