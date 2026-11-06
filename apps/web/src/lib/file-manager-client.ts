import {create,fromBinary,toBinary,type DescMessage,type MessageInitShape,type MessageShape} from '@bufbuild/protobuf';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {FileLocationSchema} from '@ai-phone/protocol/ai_phone/v1/workbench_pb';
async function result<O extends DescMessage>(response:Response,output:O):Promise<MessageShape<O>>{
 const data=new Uint8Array(await response.arrayBuffer());if(!response.ok)throw Error(fromBinary(ApiErrorSchema,data).message);return fromBinary(output,data);
}
export async function fileRpc<I extends DescMessage,O extends DescMessage>(operation:string,input:I,value:MessageInitShape<I>,output:O,signal?:AbortSignal){
 return result(await fetch('/api/files/'+operation,{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(input,create(input,value)),cache:'no-store',signal}),output);
}
export async function uploadFile(file:File,directory:string){const form=new FormData();form.append('location',new Blob([toBinary(FileLocationSchema,create(FileLocationSchema,{path:directory}))],{type:'application/x-protobuf'}));form.append('file',file);return result(await fetch('/api/files/upload',{method:'POST',body:form}),FileLocationSchema);}
