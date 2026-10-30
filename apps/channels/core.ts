import { createHash, randomInt, timingSafeEqual } from "node:crypto";
export const providers = ["wecom", "feishu", "qq", "telegram", "whatsapp"] as const;
export type Provider = typeof providers[number];
export function eventId(provider: string, appId: string, id: string) {
  const h = createHash("sha256").update(JSON.stringify([provider, appId, id])).digest("hex");
  return `${h.slice(0,8)}-${h.slice(8,12)}-5${h.slice(13,16)}-a${h.slice(17,20)}-${h.slice(20,32)}`;
}
export function permitted(allowed: string[], sender: string, bot = false) {
  return !bot && !!sender && allowed.includes(sender);
}
export function validBinding(appId: string, secret: string) {
  return /^[\w.-]{1,128}$/.test(appId) && secret.length >= 8 && secret.length <= 1024;
}
export type Pairing = { code: string; expires: number; attempts: number };
export function makePairing(now=Date.now()): Pairing {return {code:randomInt(100000,1000000).toString(),expires:now+10*60*1000,attempts:0};}
export function matchPairing(pair:Pairing|undefined,text:string,direct:boolean,now=Date.now()) {
  const match=text.trim().match(/^(?:配对|\/pair)\s+(\d{6})$/);
  if(!pair||!direct||!match||pair.expires<now||pair.attempts>=10)return false;
  pair.attempts++;
  return timingSafeEqual(Buffer.from(pair.code),Buffer.from(match[1]));
}
