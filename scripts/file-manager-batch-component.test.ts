import {test,expect} from 'bun:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';

test('file manager sends one native batch and retains only failed move receipts',()=>{
 const candidate=resolve(process.env.AI_PHONE_TEST_CANDIDATE_ROOT||join(import.meta.dir,'..'));
 const cwd=mkdtempSync(join(tmpdir(),'aiphone-file-batch-'));
 try{
  const env:Record<string,string>={AI_PHONE_TEST_CANDIDATE_ROOT:candidate};
  for(const key of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP'])if(process.env[key])env[key]=process.env[key]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file',join(import.meta.dir,'fixtures/file-manager-batch.ts')],{cwd,env,timeout:10000,stdout:'pipe',stderr:'pipe'});
  expect(result.exitCode,result.stderr.toString()).toBe(0);
  expect(result.stdout.toString()).toBe('file manager batch fixture passed\n');
 }finally{rmSync(cwd,{recursive:true,force:true});}
});
