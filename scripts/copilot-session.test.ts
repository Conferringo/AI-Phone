import {expect,test} from 'bun:test';
test('PCM → protobuf voice HTTP → correction → durable chat; device switching and barge-in preserve the session',async()=>{
  const child=Bun.spawn([process.execPath,'scripts/fixtures/copilot-session.ts'],{cwd:import.meta.dir+'/..',stdout:'pipe',stderr:'pipe'});
  const [code,stdout,stderr]=await Promise.all([child.exited,new Response(child.stdout).text(),new Response(child.stderr).text()]);
  expect({code,stderr}).toEqual({code:0,stderr:''});expect(JSON.parse(stdout)).toMatchObject({passed:true,realMicrophoneOrProvider:false});
});
