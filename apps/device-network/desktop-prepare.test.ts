import {expect, test} from 'bun:test';
import {existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';

const hostTest=process.platform==='win32'?test.skip:test;
const desktopId='11111111-1111-4111-8111-111111111111';
const nativeProgram=join(process.env.CARGO_TARGET_DIR||join(import.meta.dir,'../../target'),'debug',process.platform==='win32'?'host-resources.exe':'host-resources');

function fixture(work:(root:string,call:(kind?:'phone'|'desktop')=>any)=>void){
 const root=mkdtempSync(join(tmpdir(),'ai-phone-bind-prepare-'));
 const preload=join(root,'deny-effects.ts');
 writeFileSync(preload,`import {mock} from 'bun:test';
const deny=()=>{throw Error('Unmocked external effect forbidden')};
const processModule={spawn:deny,spawnSync:deny,exec:deny,execSync:deny,execFile:deny,execFileSync:deny,fork:deny};
mock.module('node:child_process',()=>processModule);
mock.module('child_process',()=>processModule);
mock.module('node:dns/promises',()=>({lookup:deny}));
mock.module('node:https',()=>({request:deny}));
const spawn=Bun.spawn.bind(Bun);
Bun.spawn=((args,options)=>{if(!Array.isArray(args)||args[0]!==process.env.AI_PHONE_NATIVE_HOST_PROGRAM||args[1]!=='--prepare-device-network')return deny();return spawn(args,options);});
Bun.spawnSync=deny;globalThis.fetch=deny;
`);
 const call=(kind:'phone'|'desktop'='desktop')=>{
  const env:Record<string,string>={AI_PHONE_PROJECT_ROOT:root,AI_PHONE_NATIVE_HOST_PROGRAM:nativeProgram};
  for(const name of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP'])if(process.env[name])env[name]=process.env[name]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file','--preload',preload,join(import.meta.dir,'control.ts')],{
   cwd:root,env,stdin:new TextEncoder().encode(JSON.stringify({operation:'prepare',kind,deviceId:kind==='desktop'?desktopId:'phone-local-01'})),stdout:'pipe',stderr:'pipe',timeout:10000,
  });
  const text=result.stdout.toString()+result.stderr.toString();
  expect(text).not.toContain('Unmocked external effect');
  expect(text).not.toContain('private-existing-state');
  return {code:result.exitCode,value:JSON.parse(result.stdout.toString())};
 };
 try{work(root,call);}finally{rmSync(root,{recursive:true,force:true});}
}

hostTest('desktop preparation creates a host-owned optional core bind before any container call',()=>fixture((root,call)=>{
 expect(call()).toEqual({code:0,value:{prepared:true}});
 const core=join(root,'.tools/mihomo/linux-arm64');
 expect(statSync(core).isDirectory()).toBe(true);
 if(process.getuid)expect(statSync(core).uid).toBe(process.getuid());
 expect(existsSync(join(core,'mihomo'))).toBe(false);
 const inode=statSync(core).ino;
 expect(call().code).toBe(0);
 expect(statSync(core).ino).toBe(inode);
}));

hostTest('preparation preserves an existing legacy core and private network configuration',()=>fixture((root,call)=>{
 const core=join(root,'.tools/mihomo/linux-arm64');mkdirSync(core,{recursive:true,mode:0o700});
 const binary=join(core,'mihomo');writeFileSync(binary,'private-existing-state',{mode:0o700});
 const folder=join(root,'data/device-network',desktopId);mkdirSync(folder,{recursive:true});
 const state=join(folder,'source.json');writeFileSync(state,'{"saved":"private-existing-state"}',{mode:0o600});
 const before=statSync(binary),directory=statSync(core);
 expect(call().code).toBe(0);
 expect(readFileSync(binary,'utf8')).toBe('private-existing-state');
 expect(readFileSync(state,'utf8')).toBe('{"saved":"private-existing-state"}');
 expect(statSync(binary).ino).toBe(before.ino);
 expect(statSync(binary).mode).toBe(before.mode);
 expect(statSync(core).mode).toBe(directory.mode);
}));

hostTest('a blocking cache file fails preparation without replacing its contents',()=>fixture((root,call)=>{
 mkdirSync(join(root,'.tools'));
 const blocker=join(root,'.tools/mihomo');writeFileSync(blocker,'private-existing-state');
 const result=call();expect(result.code).not.toBe(0);expect(typeof result.value.error).toBe('string');
 expect(readFileSync(blocker,'utf8')).toBe('private-existing-state');
}));

hostTest('phone preparation does not create a desktop core cache',()=>fixture((root,call)=>{
 expect(call('phone')).toEqual({code:0,value:{prepared:true}});
 expect(existsSync(join(root,'.tools'))).toBe(false);
}));
