import {test,expect} from 'bun:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
test('actual workspace loads older protobuf pages while its background task remains independent',()=>{
 const source=resolve(process.env.AI_PHONE_TEST_SOURCE_ROOT||join(import.meta.dir,'..'));
 const candidate=resolve(process.env.AI_PHONE_TEST_CANDIDATE_ROOT||source);
 const cwd=mkdtempSync(join(tmpdir(),'aiphone-window-component-'));
 try{
  const env:Record<string,string>={AI_PHONE_TEST_SOURCE_ROOT:source,AI_PHONE_TEST_CANDIDATE_ROOT:candidate,AI_PHONE_TEST_CONSOLE:process.env.AI_PHONE_TEST_CONSOLE||join(candidate,'apps/web/src/app/phone/phone-console.tsx'),AI_PHONE_TEST_PROTOBUF:Bun.resolveSync('@bufbuild/protobuf',source)};
  for(const key of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP'])if(process.env[key])env[key]=process.env[key]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file',join(import.meta.dir,'fixtures/background-conversations.ts'),'window'],{cwd,env,timeout:10000,stdout:'pipe',stderr:'pipe'});
  expect(result.exitCode,result.stderr.toString()).toBe(0);expect(result.stdout.toString()).toBe('background conversation fixture passed\n');
 }finally{rmSync(cwd,{recursive:true,force:true});}
});
