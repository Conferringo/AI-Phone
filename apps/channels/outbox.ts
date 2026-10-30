import {createHash} from 'node:crypto';
import type {Provider} from './core';
export type DeliveryStage='queued'|'running'|'sending'|'delivered'|'unknown'|'canceled'|'failed';
export function ownerTarget(provider:Provider,paired:string[]):Record<string,string>|null {
 const owner=paired[0];if(!owner)return null;
 return provider==='wecom'||provider==='telegram'||provider==='whatsapp'?{chat:owner}:provider==='feishu'?{openId:owner}:{scope:'c2c',targetId:owner};
}
export function payloadHash(text:string,png:Uint8Array){return createHash('sha256').update(text).update('\0').update(png).digest('hex');}
export function isScreenshotRequest(text:string){return /^(?:请|帮我)?\s*(?:发(?:一下|一张)?|给我看(?:一下)?)?\s*(?:当前)?(?:手机|屏幕)?(?:截图|截屏)(?:给我|看看)?[。！!？?\s]*$/u.test(text.trim())||/^(?:phone |screen )?screenshot[.!?\s]*$/i.test(text.trim());}
export function wantsScreenshot(text:string){return /截图|截屏|screenshot/i.test(text);}
export function validPng(png:Uint8Array){return png.length>=8&&png.length<=4*1024*1024&&Buffer.from(png.subarray(0,8)).equals(Buffer.from([137,80,78,71,13,10,26,10]));}
/** Checkpoint before each external effect. An uncertain send is never automatically replayed. */
export async function deliverOnce(job:{stage:DeliveryStage;textSent?:boolean;imageSent?:boolean;png?:string;error?:string},text:string,send:(text:string)=>Promise<unknown>,sendImage:(bytes:Buffer)=>Promise<unknown>,persist:()=>void){
 if(job.stage==='sending'||job.stage==='unknown'||job.stage==='delivered'||job.stage==='canceled'||job.stage==='failed')return;
 job.stage='sending';persist();
 try{
  if(text&&!job.textSent){await send(text);job.textSent=true;persist();}
  if(job.png&&!job.imageSent){await sendImage(Buffer.from(job.png,'base64'));job.imageSent=true;persist();}
  job.stage='delivered';job.error='';delete job.png;persist();
 }catch{job.stage='unknown';job.error=job.textSent?'文字已发出，后续图片结果未知，请在原应用核对':'平台未确认送达，请检查平台权限、频率限制和原应用';delete job.png;persist();}
}
export function nextPending<T extends {id:string}>(jobs:T[],previous:string):T|undefined {
 return jobs.length?jobs[(jobs.findIndex(job=>job.id===previous)+1)%jobs.length]:undefined;
}
