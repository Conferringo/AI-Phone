import {test,expect} from 'bun:test';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,existsSync,chmodSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BrowserSession,browserSelection,sessionProfile} from './whatsapp-runtime';
const image='ai-phone-bundle/desktop:sha-'+'a'.repeat(64);
function fixture(){
 const root=mkdtempSync(join(tmpdir(),'aiphone-runtime-')),python=join(root,'python'),helper=join(root,'scripts/channel_browser.py');
 mkdirSync(join(root,'scripts'));writeFileSync(python,'fixture',{mode:0o755});writeFileSync(helper,'fixture');
 const env={AI_PHONE_PYTHON:python,AI_PHONE_DESKTOP_IMAGE:image};
 return {root,env,close(){rmSync(root,{recursive:true,force:true})}};
}
test('bundled fresh accounts select the owned container without a host browser lookup',()=>{
 const f=fixture();try{
  const selected=browserSelection(f.root,'one',f.env,()=>{throw Error('host lookup')});
  expect(selected.mode).toBe('container');
  const runtime=new BrowserSession({root:f.root,sessionId:'one',proxyUrl:'',signal:new AbortController().signal},{env:f.env});
  expect(runtime.options.puppeteer).toMatchObject({pipe:true,ignoreDefaultArgs:true,headless:true});expect(runtime.options.puppeteer?.args).toContain('--user-data-dir='+sessionProfile(f.root,'one'));
  expect(existsSync(join(f.root,'data'))).toBe(false);
 }finally{f.close()}
});
test('legacy profiles retain their host browser while owned profiles cannot fall back',()=>{
 const f=fixture();try{
  const profile=sessionProfile(f.root,'one');mkdirSync(profile,{recursive:true});writeFileSync(join(profile,'account'),'keep');
  expect(browserSelection(f.root,'one',f.env,()=>'/original/browser').mode).toBe('host');
  writeFileSync(join(profile,'.aiphone-runtime.json'),'{}');
  expect(()=>browserSelection(f.root,'one',{},()=>'/original/browser')).toThrow();
  expect(browserSelection(f.root,'one',f.env,()=>'/original/browser').mode).toBe('container');
 }finally{f.close()}
});
test('invalid bundled components fail and loopback proxies are not silently bypassed',()=>{
 const f=fixture();try{
  for(const env of [{...f.env,AI_PHONE_DESKTOP_IMAGE:'ai-phone-bundle/desktop:latest'},{...f.env,AI_PHONE_PYTHON:'relative'}])expect(()=>browserSelection(f.root,'one',env)).toThrow();
  rmSync(join(f.root,'scripts/channel_browser.py'));expect(()=>browserSelection(f.root,'one',f.env)).toThrow();
  writeFileSync(join(f.root,'scripts/channel_browser.py'),'fixture');
  for(const proxyUrl of ['http://127.0.0.1:7897','http://localhost:7897','http://[::1]:7897','http://[::]:7897','http://0.0.0.0:7897'])expect(()=>new BrowserSession({root:f.root,sessionId:'one',proxyUrl,signal:new AbortController().signal},{env:f.env})).toThrow('回环');
  expect(()=>browserSelection(f.root,'one',{...f.env,AI_PHONE_PYTHON:f.root})).toThrow();
  if(process.platform!=='win32'){chmodSync(f.env.AI_PHONE_PYTHON,0o644);expect(()=>browserSelection(f.root,'one',f.env)).toThrow();}
 }finally{f.close()}
});
test('container close observes actual ownership before erase and never exposes proxy credentials in args',async()=>{
 const f=fixture(),calls:any[]=[];try{
  const runtime=new BrowserSession({root:f.root,sessionId:'one',proxyUrl:'http://user:secret@proxy.example:7897',signal:new AbortController().signal},{env:f.env,run:async(file,args,options)=>{calls.push({file,args,env:options.env});return {stdout:JSON.stringify({stopped:true,erased:args.at(-1)==='--erase'})}}});
  await expect(runtime.erase()).rejects.toThrow();
  await runtime.close({destroy:async()=>{throw Error('CDP already lost')}});await runtime.erase();
  await runtime.erase();
  expect(calls.map(c=>c.args.at(-1))).toEqual(['--stop','--erase']);
  expect(calls[0].env.AI_PHONE_BROWSER_OWNER).toMatch(/^[0-9a-f]{64}$/);expect(calls[0].env.AI_PHONE_BROWSER_OWNER).toBe(calls[1].env.AI_PHONE_BROWSER_OWNER);
  expect(runtime.options.puppeteer?.args?.join(' ')).not.toContain('secret');expect(runtime.options.proxyAuthentication?.password).toBe('secret');
 }finally{f.close()}
});
test('explicit host overrides remain available, but cannot convert a container-owned profile',()=>{
 const f=fixture();try{
  const env={...f.env,AI_PHONE_WHATSAPP_CHROMIUM:f.env.AI_PHONE_PYTHON};
  expect(browserSelection(f.root,'one',env)).toEqual({mode:'host',executable:f.env.AI_PHONE_PYTHON});
  const profile=sessionProfile(f.root,'one');mkdirSync(profile,{recursive:true});writeFileSync(join(profile,'.aiphone-runtime.json'),'{}');
  expect(browserSelection(f.root,'one',env).mode).toBe('container');
  for(const id of ['','../other','x'.repeat(129)])expect(()=>sessionProfile(f.root,id)).toThrow();
 }finally{f.close()}
});
test('corrupt helper responses do not permit account erasure',async()=>{
 const f=fixture();try{
  for(const stdout of ['null','[]','bad','{"stopped":false}']){
   const runtime=new BrowserSession({root:f.root,sessionId:'one',proxyUrl:'',signal:new AbortController().signal},{env:f.env,run:async()=>({stdout})});
   await expect(runtime.close({destroy:async()=>{}})).rejects.toThrow();await expect(runtime.erase()).rejects.toThrow();
  }
  const calls:string[]=[];
  const runtime=new BrowserSession({root:f.root,sessionId:'one',proxyUrl:'',signal:new AbortController().signal},{env:f.env,run:async(_file,args)=>{calls.push(args.at(-1)!);return {stdout:'{"stopped":true}'};}});
  await runtime.close({destroy:async()=>{}});await expect(runtime.erase()).rejects.toThrow();await expect(runtime.erase()).rejects.toThrow();
  expect(calls).toEqual(['--stop','--erase']);
 }finally{f.close()}
});
test('unknown container shutdown preserves account state and cannot unlock erase',async()=>{
 const f=fixture();try{
  const runtime=new BrowserSession({root:f.root,sessionId:'one',proxyUrl:'',signal:new AbortController().signal},{env:f.env,run:async()=>({stdout:'{"stopped":false}'})});
  await expect(runtime.close({destroy:async()=>{}})).rejects.toThrow();await expect(runtime.erase()).rejects.toThrow();
 }finally{f.close()}
});
test('SDK auth revocation defers deletion until the runtime has confirmed close',async()=>{
 const f=fixture();try{
  const profile=sessionProfile(f.root,'one');mkdirSync(profile,{recursive:true});writeFileSync(join(profile,'account'),'keep');
  const runtime=new BrowserSession({root:f.root,sessionId:'one',proxyUrl:'',signal:new AbortController().signal},{env:f.env,hostBrowser:()=>'/legacy'});
  await runtime.options.authStrategy.logout();expect(runtime.revoked).toBe(true);expect(existsSync(profile)).toBe(true);
  await runtime.close({destroy:async()=>{}});expect(existsSync(profile)).toBe(true);await runtime.erase();expect(existsSync(profile)).toBe(false);
 }finally{f.close()}
});
