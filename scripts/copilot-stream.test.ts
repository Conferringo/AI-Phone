import {expect,test} from 'bun:test';
test('live PCM and incremental transcripts retain ordered final-only chat across focus and reconnect',async()=>{
  const child=Bun.spawn([process.execPath,'scripts/fixtures/copilot-stream.ts'],{cwd:import.meta.dir+'/..',stdout:'pipe',stderr:'pipe'});
  const [code,stdout,stderr]=await Promise.all([child.exited,new Response(child.stdout).text(),new Response(child.stderr).text()]);
  expect({code,stderr}).toEqual({code:0,stderr:''});expect(JSON.parse(stdout)).toMatchObject({passed:true,realMicrophoneOrProvider:false,incrementalBeforeCommit:true,reconnectPreservesSession:true});
});
