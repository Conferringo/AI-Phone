import {expect,test} from 'bun:test';
import {createServer} from 'node:http';
import {beginAudio} from './audio-transport';
test('an idle audio connection sends a complete silent frame before any guest sound',async()=>{
 let closed=false;
 const server=createServer((req,res)=>{res.on('close',()=>{closed=true});beginAudio(res)});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 const address=server.address();if(!address||typeof address==='string')throw Error('no listener');
 const abort=new AbortController();
 try{const res=await fetch(`http://127.0.0.1:${address.port}`,{signal:AbortSignal.any([abort.signal,AbortSignal.timeout(1000)])});
  expect(res.headers.get('x-sample-format')).toBe('s16le');const reader=res.body!.getReader();
  expect([...(await reader.read()).value!]).toEqual([0,0,0,0]);await reader.cancel();
  for(let n=0;n<20&&!closed;n++)await Bun.sleep(5);expect(closed).toBe(true);
 }finally{abort.abort();server.closeAllConnections();server.close();}
});
