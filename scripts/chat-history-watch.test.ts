import {expect,test} from 'bun:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';

// Closed HTTP/protobuf fixtures use the real reader and snapshot, without DOM or PG claims.
test.each(['tail','page','canceled'])('history head watch makes progress: %s',mode=>{
 const sourceRoot=resolve(process.env.AI_PHONE_TEST_SOURCE_ROOT||join(import.meta.dir,'..'));
 const candidateRoot=resolve(process.env.AI_PHONE_TEST_CANDIDATE_ROOT||join(import.meta.dir,'..'));
 const cwd=mkdtempSync(join(tmpdir(),'aiphone-history-watch-'));
 try{
  const env:Record<string,string>={AI_PHONE_TEST_SOURCE_ROOT:sourceRoot,AI_PHONE_TEST_CANDIDATE_ROOT:candidateRoot,AI_PHONE_TEST_PROTOBUF:Bun.resolveSync('@bufbuild/protobuf',sourceRoot)};
  for(const key of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP','AI_PHONE_TEST_HISTORY_READER'])if(process.env[key])env[key]=process.env[key]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file',join(import.meta.dir,'fixtures/chat-history-watch.ts'),mode],{cwd,env,timeout:10000,stdout:'pipe',stderr:'pipe'});
  expect(result.exitCode,result.stderr.toString()).toBe(0);
  expect(result.stdout.toString()).toBe('history watch fixture passed\n');
 }finally{rmSync(cwd,{recursive:true,force:true});}
});
