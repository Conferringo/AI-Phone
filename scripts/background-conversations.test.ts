import {expect,test} from 'bun:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';

test('device focus keeps independent conversation transport and background progress',()=>{
 const sourceRoot=resolve(process.env.AI_PHONE_TEST_SOURCE_ROOT||join(import.meta.dir,'..'));
 const consolePath=resolve(process.env.AI_PHONE_TEST_CONSOLE||join(sourceRoot,'apps/web/src/app/phone/phone-console.tsx'));
 const cwd=mkdtempSync(join(tmpdir(),'aiphone-background-conversations-'));
 try{
  const env:Record<string,string>={
   AI_PHONE_TEST_SOURCE_ROOT:sourceRoot,AI_PHONE_TEST_CONSOLE:consolePath,
   AI_PHONE_TEST_PROTOBUF:Bun.resolveSync('@bufbuild/protobuf',sourceRoot),
  };
  for(const key of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP'])if(process.env[key])env[key]=process.env[key]!;
  const output=Bun.spawnSync([process.execPath,'--no-env-file',join(import.meta.dir,'fixtures/background-conversations.ts')],{cwd,env,timeout:10000,stdout:'pipe',stderr:'pipe'});
  expect(output.exitCode,output.stderr.toString()).toBe(0);
  expect(output.stdout.toString()).toBe('background conversation fixture passed\n');
 }finally{rmSync(cwd,{recursive:true,force:true});}
});
