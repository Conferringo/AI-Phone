import {describe,test,expect} from 'bun:test';
import {makeConfig,publicAddress,subscriptionUrl} from './config';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';

const node={name:'test',type:'ss',server:'node.example.com',port:443,cipher:'aes-128-gcm',password:'private-node-password'};
const config=(extra:object={},kind:'phone'|'desktop'='phone')=>makeConfig(JSON.stringify({proxies:[node],...extra}),'private-controller-secret',kind);

describe('subscription isolation',()=>{
 test('rejects loopback, local, reserved and mapped addresses',()=>{
  for(const ip of ['127.0.0.1','10.0.0.1','172.16.0.1','192.168.1.1','169.254.169.254','100.64.0.1','198.18.0.1','203.0.113.1','224.0.0.1','::1','fc00::1','fe80::1','ff02::1','::ffff:127.0.0.1','2001:db8::1'])expect(publicAddress(ip),ip).toBe(false);
  for(const ip of ['1.1.1.1','223.5.5.5','2606:4700:4700::1111'])expect(publicAddress(ip),ip).toBe(true);
 });
 test('HTTPS only and errors never echo the subscription token',()=>{
  for(const url of ['http://example.com/private-token','https://user:private-token@example.com','https://localhost/private-token','https://127.0.0.1/private-token','https://[::1]/private-token','https://service.local/private-token','https://example.com/#private-token']){
   let message='';try{subscriptionUrl(url);}catch(e){message=(e as Error).message;}expect(message).not.toBe('');expect(message).not.toContain('private-token');
  }
  expect(subscriptionUrl('https://example.com/subscription?token=private-token')).toContain('https://example.com/');
 });
 test('discards upstream listeners, controllers, rules and provider paths',()=>{
  const value=config({'external-controller':'0.0.0.0:1234',secret:'upstream-secret','mixed-port':9999,'proxy-providers':{bad:{path:'/private/file'}},rules:['MATCH,DIRECT'],script:'bad'});
  expect(value['external-controller']).toBe('127.0.0.1:19090');expect(value['mixed-port']).toBe(17890);expect(value.secret).toBe('private-controller-secret');expect(value.rules.at(-1)).toBe('MATCH,AI Phone');expect('proxy-providers' in value).toBe(false);expect('script' in value).toBe(false);
 });
 test('preserves provider-specific node DNS without importing general DNS settings',()=>{
  const dns=config({dns:{listen:'0.0.0.0:53',nameserver:['127.0.0.1'],'proxy-server-nameserver':['https://resolver.example.com/private-dns']}}).dns;
  expect(dns['proxy-server-nameserver']).toEqual(['https://resolver.example.com/private-dns']);expect(dns.listen).toBe('127.0.0.1:1053');expect(dns.nameserver).not.toContain('127.0.0.1');
  expect(()=>config({dns:{'proxy-server-nameserver':['file:///tmp/dns']}})).toThrow();
 });
 test('Android root and Linux core routes avoid proxying themselves',()=>{
  expect(config().tun['exclude-uid']).toEqual([0]);expect(config({},'desktop')['routing-mark']).toBe(9488);expect(config().tun['route-exclude-address']).toContain('192.168.0.0/16');
 });
 test('rejects malformed, duplicated and executable node data, allows WebSocket paths',()=>{
  for(const proxies of [[node,node],[{...node,port:0}],[{...node,type:'unknown'}],[{...node,'ca-path':'/etc/private'}],[{...node,'private_key_file':'/etc/private'}],[{...node,script:'code'}]])expect(()=>config({proxies})).toThrow();
  expect(config({proxies:[{...node,'ws-opts':{path:'/socket'}}]}).proxies.length).toBe(1);
 });
 test('bounds aliases, nesting, node count and string size',()=>{
  expect(()=>makeConfig('proxies: &a [*a]','s','phone')).toThrow();
  expect(()=>config({proxies:Array.from({length:513},(_,i)=>({...node,name:String(i)}))})).toThrow();
  expect(()=>config({proxies:[{...node,password:'a'.repeat(32769)}]})).toThrow();
  let deep:any='end';for(let i=0;i<15;i++)deep={next:deep};expect(()=>config({proxies:[{...node,options:deep}]})).toThrow();
 });
});

describe('private state and retry contract (no live devices)',()=>{
 const project=resolve(import.meta.dir,'../..');
 async function isolated(work:(dir:string,call:(request:object)=>Promise<any>)=>Promise<void>){
  const dir=mkdtempSync(tmpdir()+'/aiphone-network-test-');mkdirSync(dir+'/scripts');writeFileSync(dir+'/scripts/android-adb','#!/bin/sh\nexit 1\n',{mode:0o700});mkdirSync(dir+'/data/device-network/phone-local-01',{recursive:true});
  const call=async(request:object)=>{
   const child=Bun.spawn([process.execPath,project+'/apps/device-network/control.ts'],{cwd:dir,env:{...process.env,AI_PHONE_PROJECT_ROOT:dir},stdin:new TextEncoder().encode(JSON.stringify({deviceId:'phone-local-01',kind:'phone',...request})),stdout:'pipe',stderr:'pipe'});
   const output=await new Response(child.stdout).text();await child.exited;return JSON.parse(output);
  };
  try{await work(dir,call);}finally{rmSync(dir,{recursive:true,force:true});}
 }
 test('state omits secrets; only explicit credential read returns the saved URL',()=>isolated(async(dir,call)=>{
  writeFileSync(dir+'/data/device-network/phone-local-01/source.json',JSON.stringify({revision:4,enabled:false,subscriptionUrl:'https://example.com/private-subscription-token'}));
  writeFileSync(dir+'/data/device-network/phone-local-01/token','private-controller-secret');
  const state=await call({operation:'state'});expect(state.revision).toBe(4);expect(state.hasSubscription).toBe(true);expect(JSON.stringify(state)).not.toContain('private-');
  expect((await call({operation:'credential'})).subscriptionUrl).toContain('private-subscription-token');
 }));
 test('stale revisions and changed retry payloads fail before downloading or touching a device',()=>isolated(async(dir,call)=>{
  const request={operation:'apply',requestId:'test-request',expectedRevision:4,enabled:false,subscriptionUrl:'',useSavedSubscription:true};
  const fingerprint=createHash('sha256').update(JSON.stringify([4,false,'',true])).digest('hex');
  writeFileSync(dir+'/data/device-network/phone-local-01/source.json',JSON.stringify({revision:5,enabled:false,requestId:request.requestId,requestFingerprint:fingerprint}));
  expect((await call(request)).revision).toBe(5);
  expect((await call({...request,enabled:true})).error).toContain('不同配置');
  expect((await call({...request,requestId:'different-request'})).error).toContain('版本冲突');
  expect(JSON.parse(readFileSync(dir+'/data/device-network/phone-local-01/source.json','utf8')).revision).toBe(5);
 }));
 test('corrupt private state fails closed, not an empty reset',()=>isolated(async(dir,call)=>{
  writeFileSync(dir+'/data/device-network/phone-local-01/source.json','broken');expect((await call({operation:'credential'})).error).toContain('损坏');
 }));
 test('tampered cached core is rejected before execution',()=>isolated(async(dir,call)=>{
  mkdirSync(dir+'/config');mkdirSync(dir+'/.tools/mihomo/android-arm64',{recursive:true});
  writeFileSync(dir+'/config/mihomo-release.json',JSON.stringify({version:'test',geoip:{},assets:{'android-arm64':{binary_sha256:'0'.repeat(64)}}}));
  writeFileSync(dir+'/.tools/mihomo/android-arm64/mihomo','tampered');expect((await call({operation:'install'})).error).toContain('缓存核心校验失败');
 }));
});
