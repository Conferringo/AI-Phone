import {expect,test} from 'bun:test';
import {DesktopInputQueue} from '../apps/web/src/components/desktop-input-queue';
test('known control epochs partition undispatched text and issued values stay frozen',async()=>{
 const queue=new DesktopInputQueue(),values:any[]=[];let release!:()=>void;
 const held=new Promise<void>(resolve=>{release=resolve;});
 const dispatch=async(value:any)=>{values.push(value);if(values.length===1)await held;};
 void queue.enqueue({kind:'text',text:'issued'},'generation1/epoch3',dispatch);
 for(let i=0;i<30&&!values.length;i++)await Promise.resolve();
 expect(values.length).toBe(1);expect(Object.isFrozen(values[0])).toBe(true);
 void queue.enqueue({kind:'text',text:'before'},'generation1/epoch3',dispatch);
 void queue.enqueue({kind:'text',text:'after'},'generation1/epoch4',dispatch);
 const done=queue.enqueue({kind:'text',text:' correction'},'generation1/epoch4',dispatch);
 release();await done;
 expect(values.map(value=>value.text)).toEqual(['issued','before','after correction']);expect(values.every(Object.isFrozen)).toBe(true);
});
