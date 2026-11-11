import {authenticated} from '@/lib/session-server';
import {allowedOrigin,assistantError,assistantToken} from '@/lib/assistant-server';
export const runtime='nodejs';
export async function POST(request:Request){
 if(!allowedOrigin(request))return assistantError(403,'wrong_origin','请从工作台操作');
 const session=await authenticated(request);if(session!==true)return assistantError(session===false?401:503,'unavailable','服务暂不可用');
 const response=await fetch('http://127.0.0.1:17863/v1/files/upload',{method:'POST',headers:{authorization:'Bearer '+await assistantToken(),'content-type':request.headers.get('content-type')||''},body:request.body,duplex:'half',signal:request.signal} as RequestInit);
 return new Response(response.body,{status:response.status,headers:{'content-type':'application/x-protobuf','cache-control':'no-store'}});
}
