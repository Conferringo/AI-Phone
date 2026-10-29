// Read-only smoke against the actual Rust business service, not the retired P0 probe.
import {create,fromBinary,toBinary} from '@bufbuild/protobuf';
import {readFileSync} from 'node:fs';
import {ApiErrorSchema} from '../packages/protocol/src/gen/ai_phone/v1/system_pb';
import {RegistryRequestSchema,RegistryStateSchema} from '../packages/protocol/src/gen/ai_phone/v1/registry_pb';
const base=process.env.AI_PHONE_SMOKE_API||'http://127.0.0.1:17863';
const token=readFileSync(process.env.AI_PHONE_SMOKE_TOKEN_FILE||'data/assistant/token','utf8').trim();
async function send(body:Uint8Array,contentType='application/x-protobuf',authenticated=true){return fetch(new URL('/v1/registry/state',base),{method:'POST',headers:{'content-type':contentType,...(authenticated?{authorization:'Bearer '+token}:{})},body,signal:AbortSignal.timeout(10000)});}
function assert(condition:unknown,message:string):asserts condition{if(!condition)throw Error(message);}
const bytes=toBinary(RegistryRequestSchema,create(RegistryRequestSchema,{}));
const response=await send(bytes);assert(response.status===200,'Registry HTTP '+response.status);assert(response.headers.get('content-type')==='application/x-protobuf','Wrong response media type');
const value=fromBinary(RegistryStateSchema,new Uint8Array(await response.arrayBuffer()));
assert(value.devices.some(d=>d.id===value.activeDeviceId),'Registry did not return its active device');
for(const[body,media,auth,status,code]of[[new Uint8Array([255]),'application/x-protobuf',true,400,'invalid_protobuf'],[bytes,'application/json',true,415,'unsupported_media_type'],[bytes,'application/x-protobuf',false,401,'unauthorized']]as const){const result=await send(body,media,auth);assert(result.status===status,'Expected HTTP '+status+', got '+result.status);const error=fromBinary(ApiErrorSchema,new Uint8Array(await result.arrayBuffer()));assert(error.code===code,'Unexpected protocol error code');}
console.log('PASS: TypeScript → authenticated HTTP protobuf → Rust registry → TypeScript; malformed wire, wrong media type and anonymous requests rejected. No business data was changed.');
