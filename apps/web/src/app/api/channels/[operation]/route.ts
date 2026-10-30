import { readRequestBody } from '@/lib/request-body';
import {allowedOrigin,assistantError,assistantToken} from "@/lib/assistant-server";
import {authenticated} from "@/lib/session-server";
export const runtime="nodejs";
export async function POST(request:Request,{params}:{params:Promise<{operation:string}>}){
 const session=await authenticated(request); if(session!==true)return assistantError(session===false?401:503,session===false?"login_required":"auth_unavailable",session===false?"请先登录":"登录服务暂不可用，请重试");
 const {operation}=await params;
 if(!["list","bind","unbind","pair"].includes(operation))return assistantError(404,"not_found","操作不存在");
 if(!allowedOrigin(request))return assistantError(403,"wrong_origin","请从工作台操作");
 if(request.headers.get("content-type")!=="application/x-protobuf")return assistantError(415,"unsupported_media_type","请求格式错误");
 if(Number(request.headers.get("content-length")||0)>65536)return assistantError(413,"body_rejected","内容过大");
 try{const body=await readRequestBody(request,65536);if(body instanceof Response)return body;
 const result=await fetch("http://127.0.0.1:17864/v1/"+operation,{method:"POST",headers:{authorization:"Bearer "+await assistantToken(),"content-type":"application/x-protobuf"},body,cache:"no-store",signal:AbortSignal.timeout(30000)});
 return new Response(await result.arrayBuffer(),{status:result.status,headers:{"content-type":"application/x-protobuf","cache-control":"no-store"}});
 }catch{return assistantError(503,"channel_unavailable","应用连接服务暂不可用");}
}
