import {create,fromBinary,toBinary,type DescMessage,type MessageInitShape,type MessageShape} from '@bufbuild/protobuf';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
export async function settingsRpc<I extends DescMessage,O extends DescMessage>(operation:string,input:I,value:MessageInitShape<I>,output:O,group='settings',signal?:AbortSignal):Promise<MessageShape<O>> {
  const deadline=AbortSignal.timeout(group==='voice'?35000:group==='device-network'?225000:125000);
  const response=await fetch(`/api/${group}/${operation}`,{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(input,create(input,value)),cache:'no-store',signal:signal?AbortSignal.any([signal,deadline]):deadline});
  const bytes=new Uint8Array(await response.arrayBuffer());
  if(response.status===401&&group!=='setup')location.assign('/login');
  if(!response.ok)throw new Error(fromBinary(ApiErrorSchema,bytes).message||'请求失败');
  return fromBinary(output,bytes);
}
