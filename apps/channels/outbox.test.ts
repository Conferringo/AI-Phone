import {test,expect} from 'bun:test';
import {ownerTarget,deliverOnce,payloadHash,isScreenshotRequest,validPng,nextPending,type DeliveryStage} from './outbox';
test('proactive delivery resolves only the paired owner, never a message supplied recipient',()=>{
 expect(ownerTarget('qq',[])).toBeNull();expect(ownerTarget('qq',['owner','other'])).toEqual({scope:'c2c',targetId:'owner'});expect(ownerTarget('feishu',['ou_owner'])).toEqual({openId:'ou_owner'});expect(ownerTarget('wecom',['owner'])).toEqual({chat:'owner'});
});
test('partial image delivery becomes unknown and cannot resend the accepted text',async()=>{
 const job:{stage:DeliveryStage;png?:string;textSent?:boolean}={stage:'queued',png:Buffer.from('image').toString('base64')};let texts=0,images=0,saves=0;
 const send=async()=>{texts++},image=async()=>{images++;throw new Error('lost acknowledgement')};
 await deliverOnce(job,'hello',send,image,()=>{saves++});expect(job.stage).toBe('unknown');expect(job.textSent).toBe(true);expect(job.png).toBeUndefined();
 await deliverOnce(job,'hello',send,image,()=>{saves++});expect([texts,images]).toEqual([1,1]);expect(saves).toBeGreaterThanOrEqual(3);
});
test('successful text and image are delivered once with a checkpoint before side effects',async()=>{
 const job:{stage:DeliveryStage;png?:string}={stage:'queued',png:Buffer.from('image').toString('base64')};const log:string[]=[];
 await deliverOnce(job,'hello',async()=>{log.push('text')},async()=>{log.push('image')},()=>{log.push(job.stage)});expect(log[0]).toBe('sending');expect(log.at(-1)).toBe('delivered');expect(job.png).toBeUndefined();
 await deliverOnce(job,'hello',async()=>{throw new Error('duplicate')},async()=>{throw new Error('duplicate')},()=>{});
});
test('a process interrupted while sending is not blindly replayed',async()=>{let sent=false;await deliverOnce({stage:'sending'},'x',async()=>{sent=true},async()=>{sent=true},()=>{});expect(sent).toBe(false);});
test('image request recognition keeps complex phone tasks in the agent loop',()=>{
 expect(isScreenshotRequest('截图')).toBe(true);expect(isScreenshotRequest('发一下当前手机截图')).toBe(true);expect(isScreenshotRequest('screenshot')).toBe(true);expect(isScreenshotRequest('打开微信，回复后截图')).toBe(false);expect(isScreenshotRequest('不要截图')).toBe(false);
 expect(validPng(new Uint8Array([1,2,3]))).toBe(false);expect(payloadHash('x',new Uint8Array([1]))).not.toBe(payloadHash('x',new Uint8Array([2])));
});

test('all waiting jobs get a turn even if earlier model runs stay active',()=>{const jobs=[{id:'a'},{id:'b'},{id:'c'}];let previous='';const order=[];for(let i=0;i<6;i++){previous=nextPending(jobs,previous)!.id;order.push(previous);}expect(order).toEqual(['a','b','c','a','b','c']);expect(nextPending([],previous)).toBeUndefined();expect(nextPending(jobs,'retired')?.id).toBe('a');});
