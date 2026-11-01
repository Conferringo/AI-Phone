import{expect,test}from'bun:test';
import{resolve,join}from'node:path';
import{mkdtempSync,rmSync}from'node:fs';
import{tmpdir}from'node:os';

// Real hook/PCM logic with modeled React commits and Web Audio; no browser/audio-hardware claim.
test.each(['focus-continuity','independent-devices','device-disposal','failed-device-isolation','retry-disposal'])('device-owned playback: %s',(mode)=>{
 const root=resolve(import.meta.dir,'..'),cwd=mkdtempSync(join(tmpdir(),'aiphone-audio-'));
 try{
  const env:Record<string,string>={AI_PHONE_TEST_AUDIO_SOURCE:root};
  for(const key of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP','AI_PHONE_TEST_AUDIO_HOOK'])if(process.env[key])env[key]=process.env[key]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file',join(import.meta.dir,'fixtures/device-audio-hook.ts'),mode],{cwd,env,timeout:10000,stdout:'pipe',stderr:'pipe'});
  expect(result.exitCode,result.stderr.toString()).toBe(0);expect(result.stdout.toString()).toBe('device audio fixture passed\n');
 }finally{rmSync(cwd,{recursive:true,force:true});}
});
