import {create,fromBinary,toBinary,type DescMessage,type MessageInitShape,type MessageShape} from '@bufbuild/protobuf';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
export async function settingsRpc<I extends DescMessage,O extends DescMessage>(operation:string,input:I,value:MessageInitShape<I>,output:O,group='settings'):Promise<MessageShape<O>> {
  const response=await fetch(`/api/${group}/${operation}`,{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(input,create(input,value)),cache:'no-store',signal:AbortSignal.timeout(group==='device-network'?225000:125000)});
  const bytes=new Uint8Array(await response.arrayBuffer());
  if(response.status===401&&group!=='setup')location.assign('/login');
  if(!response.ok)throw new Error(fromBinary(ApiErrorSchema,bytes).message);
  return fromBinary(output,bytes);
}
