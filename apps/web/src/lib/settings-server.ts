import { readRequestBody } from './request-body';
import {authenticated} from './session-server';
import {allowedOrigin,assistantError,assistantToken} from './assistant-server';
export async function settingsBridge(request:Request,operation:string,group:'settings'|'setup'|'proactive'|'registry'|'desktop'|'phone-system'|'location') {
  if(!allowedOrigin(request))return assistantError(403,'wrong_origin','请从工作台操作');
  if(group!=='setup'){
    const session=await authenticated(request);
    if(session!==true)return assistantError(session===false?401:503,'unavailable',session===false?'请先登录':'服务正在恢复');
  }
  const operations=group==='location'?['state','update']:group==='phone-system'?['state','provision']:group==='desktop'?['state','observe','control','action','cursor','files','link','transfer','provision','explore-state','explore-update','message']:group==='registry'?['state','device-save','agent-save','agent-config-save','model-save','model-test','credential']:group==='proactive'?['state','update']:group==='settings'?['state','credential','test','save','rollback','runtime']:['status','owner'];
  if(!operations.includes(operation))return assistantError(404,'not_found','操作不存在');
  if(request.headers.get('content-type')!=='application/x-protobuf')return assistantError(415,'unsupported_media_type','请求格式错误');
  if(Number(request.headers.get('content-length')||0)>32768)return assistantError(413,'too_large','内容过大');
  try {
    const body=await readRequestBody(request,32768);if(body instanceof Response)return body;
    const result=await fetch(`http://127.0.0.1:17863/v1/${group}/${operation}`,{method:'POST',headers:{authorization:'Bearer '+await assistantToken(),'content-type':'application/x-protobuf'},body,cache:'no-store',signal:AbortSignal.timeout(120000)});
    return new Response(await result.arrayBuffer(),{status:result.status,headers:{'content-type':'application/x-protobuf','cache-control':'no-store'}});
  }catch{return assistantError(503,'unavailable','配置服务暂不可用，请重试');}
}
