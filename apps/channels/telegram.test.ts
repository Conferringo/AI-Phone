import {describe,expect,test} from 'bun:test';
import {startTelegram,telegramMessage,telegramIdentity} from './telegram';
import {matchPairing,permitted} from './core';
import {ownerTarget} from './outbox';
import {authorizedDevice} from './scopes';
const token='123456789:fixture_token_for_isolated_tests_only';
async function until(work:()=>boolean){const end=Date.now()+1500;while(!work()){if(Date.now()>end)throw Error('test timeout');await Bun.sleep(5);}}
describe('Telegram outbound polling adapter',()=>{
 test('screenshots cannot fall back to another Agent device',()=>{
  const state={agents:[{id:'desktop-agent',deviceIds:['desktop']},{id:'phone-agent',deviceIds:['phone']}],devices:[{id:'phone',kind:'phone',runtimeState:'managed'},{id:'desktop',kind:'desktop',runtimeState:'managed'},{id:'archived',kind:'phone',runtimeState:'archived'}]};
  expect(authorizedDevice('desktop-agent','phone',state)).toBeUndefined();
  expect(authorizedDevice('desktop-agent','desktop',state)?.kind).toBe('desktop');
  expect(authorizedDevice('phone-agent','archived',state)).toBeUndefined();
 });
 test('only private non-bot text is eligible; pairing remains required',()=>{
  const update={update_id:12,message:{message_id:4,text:'/pair 123456',chat:{id:42,type:'private'},from:{id:42,is_bot:false}}};
  expect(telegramIdentity(token)).toBe('123456789');
  expect(telegramIdentity('invalid')).toBe('');
  expect(telegramMessage(update)?.sender).toBe('42');
  expect(telegramMessage({...update,message:{...update.message,chat:{id:42,type:'group'}}})).toBeNull();
  expect(telegramMessage({...update,message:{...update.message,from:{id:42,is_bot:true}}})).toBeNull();
  expect(permitted([],'42')).toBe(false);
  expect(matchPairing({code:'123456',expires:Date.now()+10000,attempts:0},update.message.text,true)).toBe(true);
  expect(ownerTarget('telegram',['42'])).toEqual({chat:'42'});
 });
 test('checkpoints after enqueue and does not consume another service webhook',async()=>{
  let poll=0;const order:string[]=[],methods:string[]=[];
  const fetcher=(async(url:any,options:any)=>{
   const method=String(url).split('/').at(-1)!;methods.push(method);
   let result:any={};if(method==='getMe')result={id:123456789,is_bot:true};
   if(method==='getUpdates'){
    poll++;if(poll>1){await new Promise((_r,reject)=>options.signal.addEventListener('abort',()=>reject(Error('stopped')),{once:true}));}
    result=[{update_id:12,message:{message_id:4,text:'hello',chat:{id:42,type:'private'},from:{id:42}}}];
   }
   return Response.json({ok:true,result});
  }) as typeof fetch;
  const client=startTelegram({appId:'123456789',token,offset:0,fetcher,onMessage:()=>order.push('persist-enqueue'),checkpoint:o=>order.push('offset-'+o)});
  await until(()=>order.length===2);client.stop();
  expect(order).toEqual(['persist-enqueue','offset-13']);
  expect(methods).not.toContain('deleteWebhook');
  const blocked=startTelegram({appId:'123456789',token,offset:0,fetcher:(async(url:any)=>Response.json({ok:true,result:String(url).endsWith('/getMe')?{id:123456789,is_bot:true}:{url:'https://existing.example/hook'}})) as typeof fetch,onMessage:()=>{throw Error('must not poll');},checkpoint:()=>{}});
  await until(()=>blocked.state==='failed');expect(blocked.error).toContain('Webhook');blocked.stop();
 });
 test('supports bounded text and multipart screenshot sending',async()=>{
  let sent='',photo=false;
  const client=startTelegram({appId:'123456789',token,offset:0,fetcher:(async(url:any,options:any)=>{
   const method=String(url).split('/').at(-1);
   if(method==='sendMessage')sent=JSON.parse(options.body).text;
   if(method==='sendPhoto'){expect(options.body).toBeInstanceOf(FormData);photo=options.body.get('photo') instanceof Blob;}
   if(method==='getUpdates')await new Promise((_r,reject)=>options.signal.addEventListener('abort',()=>reject(Error('stopped')),{once:true}));
   return Response.json({ok:true,result:method==='getMe'?{id:123456789,is_bot:true}:{}});
  }) as typeof fetch,onMessage:()=>{},checkpoint:()=>{}});
  await client.send({target:{chat:'42'}},'x'.repeat(6000));
  await client.sendImage({target:{chat:'42'}},Buffer.from([137,80,78,71]));
  client.stop();expect(sent.length).toBeLessThanOrEqual(4000);expect(photo).toBe(true);
 });
});
