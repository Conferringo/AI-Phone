// Pure screen -> physical touch transform. Pressure is a normalized signal, not Newtons.
export function physicalPoint(x:number,y:number,rotation:number,width=720,height=1440){
 switch(rotation){case 1:return {x:width-1-y,y:x};case 2:return {x:width-1-x,y:height-1-y};case 3:return {x:y,y:height-1-x};default:return {x,y};}
}
export function interpolate(points:readonly {x:number;y:number;atMs:number}[],time:number){
 if(time<=0)return points[0];
 for(let i=1;i<points.length;i++)if(time<=points[i].atMs){const a=points[i-1],b=points[i],p=(time-a.atMs)/(b.atMs-a.atMs);return {x:Math.round(a.x+(b.x-a.x)*p),y:Math.round(a.y+(b.y-a.y)*p)};}
 return points[points.length-1];
}
