import { create,fromBinary,toBinary } from "@bufbuild/protobuf";
import { SessionRequestSchema,SessionStateSchema } from "@ai-phone/protocol/ai_phone/v1/auth_pb";
import { assistantToken } from "./assistant-server";
export const SESSION_COOKIE="ai_phone_session";
export function requestSessionToken(request:Request){return request.headers.get("cookie")?.split(";").map(v=>v.trim()).find(v=>v.startsWith(SESSION_COOKIE+"="))?.slice(SESSION_COOKIE.length+1)??"";}
export async function validSession(token:string): Promise<boolean|null>{
  if(!/^[a-f0-9]{64}$/.test(token))return false;
  try{const response=await fetch("http://127.0.0.1:17863/v1/session",{method:"POST",headers:{authorization:"Bearer "+await assistantToken(),"content-type":"application/x-protobuf"},body:toBinary(SessionRequestSchema,create(SessionRequestSchema,{token})),cache:"no-store",signal:AbortSignal.timeout(5000)});if(!response.ok)return null;return fromBinary(SessionStateSchema,new Uint8Array(await response.arrayBuffer())).authenticated;}catch{return null;}
}
export async function authenticated(request:Request){return validSession(requestSessionToken(request));}
