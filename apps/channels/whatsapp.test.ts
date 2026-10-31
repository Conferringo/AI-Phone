import {test,expect} from 'bun:test';
import {EventEmitter} from 'node:events';
import {mkdtempSync,rmSync,mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {startWhatsApp} from './whatsapp';
import {whatsappProxy,whatsappBrowser} from './whatsapp-browser';
import {BrowserConfigurationError} from './whatsapp-runtime';
import {ownerTarget} from './outbox';
class Fake extends EventEmitter{
 info={wid:{_serialized:'account@c.us'}};destroyed=0;loggedOut=0;sent:any[]=[];
 async initialize(){}async destroy(){this.destroyed++;}async logout(){this.loggedOut++;}
 async sendMessage(chat:string,content:any){this.sent.push({chat,content});return {id:{_serialized:'outgoing'}};}
}
function fixture(extra:Partial<Parameters<typeof startWhatsApp>[0]>={},dependencies:Parameters<typeof startWhatsApp>[1]={}){
 const root=mkdtempSync(join(tmpdir(),'aiphone-wa-test-')),clients:Fake[]=[],options:any[]=[];const seen:any[]=[];
 const control=startWhatsApp({root,sessionId:'owned-test',proxyUrl:'',onIdentity:()=>true,onMessage:(...v)=>seen.push(v),...extra},{browser:()=>'/fixture/chromium',client:config=>{options.push(config);const fake=new Fake();clients.push(fake);return fake;},qr:async()=>Buffer.from('fixture-qr'),...dependencies});
 return {root,control,clients,options,seen,async close(){await control.stop();rmSync(root,{recursive:true,force:true});}};
}
const flush=()=>new Promise(r=>setTimeout(r,0));
test('QR login, identity gate, private messages and image replies share the paired-owner transport',async()=>{
 const f=fixture();try{
  f.clients[0].emit('qr','private-qr');await flush();expect(f.control.state).toBe('waiting_qr');expect(f.control.qrPng.length).toBeGreaterThan(0);
  f.clients[0].emit('authenticated');expect(f.control.qrPng.length).toBe(0);expect(f.control.state).toBe('authenticating');
  f.clients[0].emit('ready');expect(f.control.state).toBe('connected');
  const message={from:'owner@lid',body:'/pair 123456',fromMe:false,isStatus:false,id:{_serialized:'received'}};
  f.clients[0].emit('message',message);f.clients[0].emit('message',{...message,from:'group@g.us'});f.clients[0].emit('message',{...message,fromMe:true});f.clients[0].emit('message',{...message,isStatus:true});expect(f.seen).toHaveLength(1);
  const target=ownerTarget('whatsapp',['owner@lid']) as {chat:string};expect(target).toEqual({chat:'owner@lid'});
  await f.control.send({id:'one',target},'reply');await f.control.sendImage({id:'two',target},Buffer.from('PNG'));
  expect(f.clients[0].sent[0]).toEqual({chat:'owner@lid',content:'reply'});expect(f.clients[0].sent[1].content.mimetype).toBe('image/png');
  expect(await f.control.logout()).toBe(true);expect(f.clients[0].loggedOut).toBe(1);expect(f.options[0].puppeteer.signal.aborted).toBe(true);
 }finally{await f.close();}
});
test('late QR work and callbacks cannot resurrect a stopped connection',async()=>{
 let resolve!:(v:Buffer)=>void;const f=fixture({}, {qr:()=>new Promise(r=>resolve=r)});try{
  f.clients[0].emit('qr','old');await f.control.stop();resolve(Buffer.from('late'));await flush();f.clients[0].emit('ready');f.clients[0].emit('message',{from:'owner@c.us',body:'late',id:{_serialized:'late'}});
  expect(f.control.qrPng.length).toBe(0);expect(f.control.state).toBe('idle');expect(f.seen).toHaveLength(0);
 }finally{await f.close();}
});
test('authentication keeps a still-rendering QR from replacing the authenticating state',async()=>{
 let resolve!:(v:Buffer)=>void;const f=fixture({}, {qr:()=>new Promise(r=>resolve=r)});
 try{f.clients[0].emit('qr','old');f.clients[0].emit('authenticated');resolve(Buffer.from('stale'));await flush();expect(f.control.state).toBe('authenticating');expect(f.control.qrPng.length).toBe(0);}finally{await f.close()}
});
test('send requires a ready connection and honors the message length boundary',async()=>{
 const f=fixture();const job={id:'boundary',target:{chat:'owner@c.us'}};
 try{
  await expect(f.control.send(job,'not ready')).rejects.toThrow('未连接');f.clients[0].emit('ready');
  for(const text of ['', 'x'.repeat(65001)])await expect(f.control.send(job,text)).rejects.toThrow('长度');
  expect(await f.control.send(job,'x'.repeat(65000))).toBe('outgoing');await f.control.stop();await expect(f.control.send(job,'stopped')).rejects.toThrow('未连接');
 }finally{await f.close()}
});
test('unpaired logout is confirmed and failed construction can still be stopped',async()=>{
 const f=fixture();try{f.clients[0].emit('qr','fixture');await flush();expect(await f.control.logout()).toBe(true);expect(f.clients[0].loggedOut).toBe(0);}finally{await f.close()}
 const broken=fixture({}, {browser:()=>{throw new BrowserConfigurationError('missing component')}});
 try{expect(broken.control.state).toBe('failed');expect(broken.control.error).toBe('missing component');await broken.control.stop();await expect(broken.control.logout()).rejects.toThrow('数据已保留');}finally{await broken.close()}
});
test('authentication supersedes an in-flight QR render and rejects duplicate identities',async()=>{
 let resolve!:(v:Buffer)=>void;const f=fixture({onIdentity:()=>false},{qr:()=>new Promise(r=>resolve=r)});try{
  f.clients[0].emit('qr','old');f.clients[0].emit('authenticated');f.clients[0].emit('ready');resolve(Buffer.from('stale'));await flush();
  expect(f.control.state).toBe('failed');expect(f.control.qrPng.length).toBe(0);expect(f.clients[0].destroyed).toBeGreaterThan(0);
 }finally{await f.close();}
});
test('network reconnection closes the old client and ignores its later events',async()=>{
 const f=fixture({}, {retryMs:1});try{
  f.clients[0].emit('ready');f.clients[0].emit('disconnected','TIMEOUT');
  for(let i=0;i<30&&f.clients.length<2;i++)await new Promise(r=>setTimeout(r,5));
  expect(f.clients).toHaveLength(2);expect(f.clients[0].destroyed).toBeGreaterThan(0);
  f.clients[0].emit('ready');expect(f.control.state).not.toBe('connected');f.clients[1].emit('ready');expect(f.control.state).toBe('connected');
 }finally{await f.close();}
});
test('storage failures stop reception instead of dispatching unsaved instructions',async()=>{
 const f=fixture({onMessage:()=>{throw Error('fixture storage failure');}});try{f.clients[0].emit('ready');f.clients[0].emit('message',{from:'owner@c.us',body:'work',fromMe:false,isStatus:false,id:{_serialized:'one'}});await flush();expect(f.control.state).toBe('failed');expect(f.clients[0].destroyed).toBeGreaterThan(0);}finally{await f.close();}
});
test('identity storage and QR failures stop the attempt without publishing a connection',async()=>{
 const identity=fixture({onIdentity:()=>{throw Error('storage unavailable')}});
 try{identity.clients[0].emit('ready');await flush();expect(identity.control.state).toBe('failed');expect(identity.clients[0].destroyed).toBe(1);}finally{await identity.close()}
 const qr=fixture({}, {qr:async()=>{throw Error('renderer failed')}});
 try{qr.clients[0].emit('qr','fixture');await flush();expect(qr.control.state).toBe('failed');expect(qr.control.qrPng.length).toBe(0);expect(qr.clients[0].destroyed).toBe(1);}finally{await qr.close()}
});
test('proxy credentials are separated from Chrome arguments and browser manifests cannot escape',()=>{
 expect(whatsappProxy('http://user:pass@127.0.0.1:7897')).toEqual({args:['--proxy-server=http://127.0.0.1:7897'],auth:{username:'user',password:'pass'}});
 for(const url of ['file:///tmp/else','https://proxy.example/path','socks5://user:pass@proxy.example:1080'])expect(()=>whatsappProxy(url)).toThrow();
 const root=mkdtempSync(join(tmpdir(),'aiphone-wa-component-'));try{mkdirSync(join(root,'.tools/whatsapp-browser'),{recursive:true});writeFileSync(join(root,'.tools/whatsapp-browser/browser.json'),JSON.stringify({format:1,executable:'../../../../bin/sh'}));expect(()=>whatsappBrowser(root)).toThrow();}finally{rmSync(root,{recursive:true,force:true});}
});
test('uncertain browser close blocks reconnect and preserves the account on logout',async()=>{
 const f=fixture({}, {retryMs:1});const profile=join(f.root,'data/channels/whatsapp/session-owned-test');
 mkdirSync(profile,{recursive:true});writeFileSync(join(profile,'account'),'keep');
 f.clients[0].destroy=async()=>{throw Error('close unknown')};
 try{
  f.clients[0].emit('ready');f.clients[0].emit('disconnected','TIMEOUT');
  await new Promise(r=>setTimeout(r,20));expect(f.clients).toHaveLength(1);expect(f.control.state).toBe('failed');
  await expect(f.control.logout()).rejects.toThrow();expect(readFileSync(join(profile,'account'),'utf8')).toBe('keep');
 }finally{await f.control.stop().catch(()=>{});rmSync(f.root,{recursive:true,force:true});}
});
test('overlapping stops await one close and ignore late initialization',async()=>{
 let finish!:()=>void;let complete!:()=>void;
 const fake=new Fake();fake.initialize=()=>new Promise(r=>complete=r);fake.destroy=()=>{fake.destroyed++;return new Promise(r=>finish=r)};
 const f=fixture({}, {client:()=>fake});
 try{
  let done=false;const first=f.control.stop().then(()=>{done=true}),second=f.control.stop();await flush();
  expect(done).toBe(false);expect(fake.destroyed).toBe(1);complete();await flush();finish();await Promise.all([first,second]);await flush();
  expect(fake.destroyed).toBe(1);expect(f.control.state).toBe('idle');
 }finally{await f.close();}
});
test('an already queued retry cannot resurrect a stopped connection',async()=>{
 const native=globalThis.setTimeout;let queued:(()=>void)|undefined;
 globalThis.setTimeout=((callback:any,delay:number,...args:any[])=>{
  if(delay===1234){queued=callback;return native(()=>{},60000);}
  return native(callback,delay,...args);
 }) as typeof setTimeout;
 const f=fixture({}, {retryMs:1234});
 try{
  f.clients[0].emit('ready');f.clients[0].emit('disconnected','TIMEOUT');await flush();expect(queued).toBeDefined();
  await f.control.stop();queued!();await flush();expect(f.clients).toHaveLength(1);expect(f.control.state).toBe('idle');
 }finally{globalThis.setTimeout=native;await f.close()}
});
test('normal initialization stays live and incoming work waits for the identity gate',async()=>{
 const f=fixture(),message={from:'owner@c.us',body:'work',fromMe:false,isStatus:false,id:{_serialized:'one'}};
 try{
  await flush();expect(f.clients[0].destroyed).toBe(0);f.clients[0].emit('message',message);expect(f.seen).toHaveLength(0);
  f.clients[0].emit('ready');f.clients[0].emit('message',message);expect(f.seen).toHaveLength(1);
 }finally{await f.close()}
 const missing=fixture();try{Object.defineProperty(missing.clients[0],'info',{value:undefined});expect(()=>missing.clients[0].emit('ready')).not.toThrow();await flush();expect(missing.control.state).toBe('failed');}finally{await missing.close()}
});
test('reconnect uses bounded backoff, resets after ready, and honors an explicit zero delay',async()=>{
 const native=globalThis.setTimeout,scheduled:{callback:()=>void;delay:number;timer:ReturnType<typeof setTimeout>}[]=[];
 const settle=()=>new Promise(resolve=>native(resolve,0));
 globalThis.setTimeout=((callback:any,delay:number,...args:any[])=>{
  if(delay<60000){const timer=native(()=>{},60000);scheduled.push({callback,delay,timer});return timer;}
  return native(callback,delay,...args);
 }) as typeof setTimeout;
 const f=fixture();let zero:ReturnType<typeof fixture>|undefined;
 try{
  f.clients[0].emit('disconnected','TIMEOUT');await settle();expect(scheduled.at(-1)?.delay).toBe(2000);scheduled.at(-1)!.callback();await settle();
  f.clients[1].emit('disconnected','TIMEOUT');await settle();expect(scheduled.at(-1)?.delay).toBe(4000);scheduled.at(-1)!.callback();await settle();
  f.clients[2].emit('ready');f.clients[2].emit('disconnected','TIMEOUT');await settle();expect(scheduled.at(-1)?.delay).toBe(2000);
  zero=fixture({}, {retryMs:0});zero.clients[0].emit('disconnected','TIMEOUT');await settle();expect(scheduled.at(-1)?.delay).toBe(0);
 }finally{globalThis.setTimeout=native;await f.close();await zero?.close();for(const item of scheduled)clearTimeout(item.timer);}
});
