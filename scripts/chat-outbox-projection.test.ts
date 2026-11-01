import {expect,test} from 'bun:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';

// A separate process confines module mocks; no browser/React-rendering claim is made.
const sourceRoot=resolve(process.env.AI_PHONE_TEST_SOURCE_ROOT||join(import.meta.dir,'..'));
const candidateRoot=resolve(import.meta.dir,'..');
const cases=[
 'slow-history','history-failure','projection-order','binding-dispose',
 'unknown-post','late-post','projection-callback-failure','invalid-receipt','snapshot-contract',
 'accepted-storage-fault','acceptance-write-reload','projection-cleanup-fault','invalid-status','invalid-media','duplicate-receipt','invalid-user-receipt','history-contract','accepted-failed-fifo','history-proof',
];
test.each(cases)('accepted chat projection: %s',(mode)=>{
 const cwd=mkdtempSync(join(tmpdir(),'aiphone-chat-projection-'));
 try{
  const env:Record<string,string>={
   AI_PHONE_TEST_SOURCE_ROOT:sourceRoot,AI_PHONE_TEST_CANDIDATE_ROOT:candidateRoot,
   AI_PHONE_TEST_PROTOBUF:Bun.resolveSync('@bufbuild/protobuf',sourceRoot),
  };
  for(const key of ['AI_PHONE_TEST_HOOK','AI_PHONE_TEST_OUTBOX'])if(process.env[key])env[key]=process.env[key]!;
  for(const key of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP'])if(process.env[key])env[key]=process.env[key]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file',join(import.meta.dir,'fixtures/chat-outbox-hook.ts'),mode],{cwd,env,timeout:10000,stdout:'pipe',stderr:'pipe'});
  expect(result.exitCode,result.stderr.toString()).toBe(0);
  expect(result.stdout.toString()).toBe('projection fixture passed\n');
 }finally{rmSync(cwd,{recursive:true,force:true});}
});
