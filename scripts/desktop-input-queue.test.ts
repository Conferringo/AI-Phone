import {expect,test} from 'bun:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
const sourceRoot=resolve(process.env.AI_PHONE_TEST_SOURCE_ROOT||join(import.meta.dir,'..'));
const component=resolve(process.env.AI_PHONE_TEST_DESKTOP_SURFACE||join(sourceRoot,'apps/web/src/components/desktop-surface.tsx'));
const cases=['burst','warm-inflight','sync-burst','key-barrier','ime','paste','focus-barrier','drag-barrier','control-barrier','late-control','unknown','merged-unknown','stop-focus','ime-stop','device-change','dispose','character-limit','unicode-byte-limit'];
test.each(cases)('desktop input queue: %s',mode=>{
 const cwd=mkdtempSync(join(tmpdir(),'aiphone-desktop-input-'));
 try{
  const env:Record<string,string>={AI_PHONE_TEST_SOURCE_ROOT:sourceRoot,AI_PHONE_TEST_DESKTOP_SURFACE:component};
  for(const key of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP'])if(process.env[key])env[key]=process.env[key]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file','--tsconfig-override',join(sourceRoot,'apps/web/tsconfig.json'),join(import.meta.dir,'fixtures/desktop-input.ts'),mode],{cwd,env,timeout:10000,stdout:'pipe',stderr:'pipe'});
  expect(result.exitCode,result.stderr.toString()).toBe(0);
  const receipt=JSON.parse(result.stdout.toString());expect(receipt.external_attempts).toBe(0);
 }finally{rmSync(cwd,{recursive:true,force:true});}
});
