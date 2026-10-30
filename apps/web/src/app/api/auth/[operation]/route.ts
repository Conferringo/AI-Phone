import { readRequestBody } from '@/lib/request-body';
import { create,fromBinary,toBinary } from "@bufbuild/protobuf";
import { SessionRequestSchema,SessionStateSchema,LoginResultSchema } from "@ai-phone/protocol/ai_phone/v1/auth_pb";
import { allowedOrigin,assistantToken,assistantError } from "@/lib/assistant-server";
import { SESSION_COOKIE,requestSessionToken } from "@/lib/session-server";
export const runtime="nodejs";
export async function POST(request:Request,{params}:{params:Promise<{operation:string}>}){
  const {operation}=await params;if(!["login","enter","logout"].includes(operation))return assistantError(404,"not_found","操作不存在");
  if(!allowedOrigin(request))return assistantError(403,"wrong_origin","请从工作台登录");
  try{
    if(Number(request.headers.get("content-length")??0)>4096)return assistantError(413,"body_rejected","内容过大");
    if(operation!=="logout"&&request.headers.get("content-type")!=="application/x-protobuf")return assistantError(415,"unsupported_media_type","请求格式错误");
    const body=operation!=="logout"?await readRequestBody(request,4096):toBinary(SessionRequestSchema,create(SessionRequestSchema,{token:requestSessionToken(request)}));
    if(body instanceof Response)return body;
    if(body.length>4096)return assistantError(413,"body_rejected","内容过大");
    const result=await fetch("http://127.0.0.1:17863/v1/"+operation,{method:"POST",headers:{authorization:"Bearer "+await assistantToken(),"content-type":"application/x-protobuf"},body,cache:"no-store",signal:AbortSignal.timeout(10000)});
    const bytes=new Uint8Array(await result.arrayBuffer());if(!result.ok)return new Response(bytes,{status:result.status,headers:{"content-type":"application/x-protobuf"}});
    const login=operation!=="logout"?fromBinary(LoginResultSchema,bytes):null;
    return new Response(toBinary(SessionStateSchema,create(SessionStateSchema,{authenticated:!!login,username:login?.username??""})),{headers:{"content-type":"application/x-protobuf","cache-control":"no-store","set-cookie":`${SESSION_COOKIE}=${login?.token??""}; Path=/; HttpOnly; SameSite=Strict; Max-Age=${login?43200:0}`}});
  }catch{return assistantError(503,"auth_unavailable","登录服务暂不可用，请重试");}
}
