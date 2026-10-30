import {test,expect} from 'bun:test';
import {simplifyStroke} from '../apps/web/src/lib/route-stroke';
test('route drawing retains corners, endpoints and meaningful distance',()=>{
 const points=[...Array.from({length:101},(_,x)=>({x,y:0})),...Array.from({length:100},(_,i)=>({x:100,y:i+1}))];
 const out=simplifyStroke(points);
 expect(out).toEqual([{x:0,y:0},{x:100,y:0},{x:100,y:100}]);
});
test('dense freehand routes stay within the wire limit and retain closed endpoints',()=>{
 const points=Array.from({length:5000},(_,i)=>({x:Math.cos(i/4999*2*Math.PI)*200,y:Math.sin(i/4999*2*Math.PI)*200}));
 const out=simplifyStroke(points);expect(out.length).toBeGreaterThan(8);expect(out.length).toBeLessThanOrEqual(200);expect(out[0]).toEqual(points[0]);expect(out.at(-1)).toEqual(points.at(-1));
 const dense=Array.from({length:5000},(_,i)=>({x:i,y:i%2*100}));expect(simplifyStroke(dense).length).toBeLessThanOrEqual(200);
});
