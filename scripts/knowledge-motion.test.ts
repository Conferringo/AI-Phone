import {test,expect} from 'bun:test';
import {createKnowledgeGraphMotion,placeKnowledgeLabels} from '../apps/web/src/components/knowledge/knowledgeGraphMotion';
import {knowledgeLayout} from '../apps/web/src/components/knowledge/knowledgeGraphLayout';
function fixture(){
 let time=0,hidden=false,reduced=false,next=0;const frames=new Map<number,FrameRequestCallback>();const curves:number[][]=[];
 const context={setTransform(){},clearRect(){},beginPath(){},moveTo(){},bezierCurveTo(...v:number[]){curves.push(v);},stroke(){},arc(){},fill(){}};
 const canvas={width:0,height:0,getContext:()=>context} as unknown as HTMLCanvasElement;
 const a={style:{transform:''}},b={style:{transform:''}};const elements=new Map([['a',a as unknown as HTMLElement],['b',b as unknown as HTMLElement]]);
 const motion=createKnowledgeGraphMotion({surface:canvas,elements,model:{nodes:[{id:'a',title:'A',x:-80,y:0},{id:'b',title:'B',x:80,y:0}],links:[{source_id:'a',target_id:'b'},{source_id:'missing',target_id:'b'}]}},{requestFrame:callback=>{frames.set(++next,callback);return next;},cancelFrame:id=>{frames.delete(id);},now:()=>time,hidden:()=>hidden,reduced:()=>reduced});
 const step=(ms=16)=>{time+=ms;const pending=[...frames.values()];frames.clear();for(const callback of pending)callback(time);};
 return {motion,frames,step,a,b,canvas,curves,setHidden:(value:boolean)=>{hidden=value;},setReduced:(value:boolean)=>{reduced=value;}};
}
test('knowledge graph retains node, pan, zoom and reset controls without reallocating on pointer movement',()=>{
 const f=fixture();f.motion.resize(640,400,2);f.step();const start=f.a.style.transform;expect(f.curves.length).toBeGreaterThan(0);
 expect(f.frames.size).toBe(0);f.motion.move('a',20,10);f.step();expect(f.a.style.transform).not.toBe(start);
 const moved=f.a.style.transform;f.motion.panBy(30,-20);f.step();expect(f.a.style.transform).not.toBe(moved);
 const width=f.canvas.width;f.motion.target(300,220);for(let i=0;i<120;i++)f.step();expect(f.canvas.width).toBe(width);expect(f.frames.size).toBe(0);
 f.motion.home();f.step();expect(f.a.style.transform).not.toContain('NaN');f.motion.dispose();
});
test('reduced motion, hidden state and disposal stop animation and safely handle absent edge targets',()=>{
 const f=fixture();f.motion.resize(640,400,1);f.step();f.motion.highlight('a');f.step();expect(f.frames.size).toBe(1);
 f.setHidden(true);f.motion.sync();expect(f.frames.size).toBe(0);
 f.setHidden(false);f.setReduced(true);f.motion.sync();f.step();expect(f.frames.size).toBe(0);
 f.motion.zoomBy(Number.NaN);f.motion.target(Infinity,Infinity);f.step();expect(f.a.style.transform).not.toContain('NaN');
 f.motion.dispose();f.motion.panBy(20,20);expect(f.frames.size).toBe(0);f.motion.dispose();
});
test('long labels remain separate and inside the desktop and narrow graph viewports',()=>{
 const model=knowledgeLayout(Array.from({length:14},(_,i)=>({id:String(i),title:'A long document title '+i})),[]);
 const radius=model.nodes.reduce((max,node)=>Math.max(max,Math.abs(node.x),Math.abs(node.y)),150);
 for(const [width,height] of [[800,450],[350,450]]){
  const scale=Math.max(.2,Math.min(1.1,width/(radius*2+150),height/(radius*2+110)));
  const points=model.nodes.map(node=>({id:node.id,x:width/2+node.x*scale,y:height/2+node.y*scale,width:140,height:37}));
  const boxes=[...placeKnowledgeLabels(points,width,height).values()];
  expect(boxes).toHaveLength(14);
  boxes.forEach((box,index)=>{
   expect(box.x).toBeGreaterThanOrEqual(0);expect(box.y).toBeGreaterThanOrEqual(0);
   expect(box.x+box.width).toBeLessThanOrEqual(width);expect(box.y+box.height).toBeLessThanOrEqual(height);
   for(const other of boxes.slice(0,index)){
    const overlaps=Math.min(box.x+box.width,other.x+other.width)>Math.max(box.x,other.x)&&Math.min(box.y+box.height,other.y+other.height)>Math.max(box.y,other.y);
    expect(overlaps).toBe(false);
   }
  });
 }
});
