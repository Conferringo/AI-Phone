import { readRequestBody } from '@/lib/request-body';
import {phoneTarget} from "@/lib/phone-target-server";
import {PhoneScopeSchema} from "@ai-phone/protocol/ai_phone/v1/phone_runtime_pb";
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import { ApiErrorSchema } from "@ai-phone/protocol/ai_phone/v1/system_pb";
import { authenticated } from "@/lib/session-server";
import { allowedOrigin } from "@/lib/assistant-server";

export const runtime = "nodejs";
function error(status: number, code: string, message: string) {
  return new Response(toBinary(ApiErrorSchema,create(ApiErrorSchema,{code,message})),{status,headers:{"content-type":"application/x-protobuf","cache-control":"no-store"}});
}

// P2 local-only bridge. The pairing token never reaches the browser.
export async function POST(request: Request, {params}: {params: Promise<{operation:string}>}) {
  const {operation}=await params;
  const session=await authenticated(request); if(session!==true)return error(session===false?401:503,session===false?"login_required":"auth_unavailable",session===false?"请先登录":"登录服务暂不可用，请重试");
  if(!["observe","orientation","action","control","cursor"].includes(operation))return error(404,"not_found","没有这项手机操作");
  // Next's internal Request URL may normalize 127.0.0.1 to localhost.
  if(!allowedOrigin(request))return error(403,"wrong_origin","请从工作台操作手机");
  if(request.headers.get("content-type")!=="application/x-protobuf")return error(415,"unsupported_media_type","需要 protobuf 请求");
  const length=Number(request.headers.get("content-length")??"0");
  if(length>32768)return error(413,"body_rejected","请求内容过大");
  try{
    const body=await readRequestBody(request,32768);if(body instanceof Response)return body;
    const {token,runner}=await phoneTarget(fromBinary(PhoneScopeSchema,body).phoneId);
    const response=await fetch(`http://127.0.0.1:${runner}/v1/${operation}`,{method:"POST",headers:{authorization:"Bearer "+token,"content-type":"application/x-protobuf"},body,signal:AbortSignal.any([request.signal,AbortSignal.timeout(30000)]),cache:"no-store"});
    return new Response(await response.arrayBuffer(),{status:response.status,headers:{"content-type":"application/x-protobuf","cache-control":"no-store"}});
  }catch{return error(503,"runner_unavailable","手机服务暂不可用，请检查运行状态后重试");}
}
