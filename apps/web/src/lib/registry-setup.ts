import presets from '../../../../config/model-providers.json';
import {create,type MessageShape} from '@bufbuild/protobuf';
import {ModelDraftSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
import {RegistryStateSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';

type Draft=MessageShape<typeof ModelDraftSchema>;
export const modelPresets=presets;

// Explicit selection creates a fresh connection; existing secrets/profiles are never overwritten.
export function presetDraft(preset:string):Draft {
 const profile=presets.find(p=>p.id===preset)??presets.find(p=>p.id==='custom');
 return create(ModelDraftSchema,profile);
}
export function modelPreset(draft:Pick<Draft,'baseUrl'|'provider'>):string {
 const base=draft.baseUrl.replace(/\/+$/,'');
 return presets.find(p=>p.id!=='custom'&&p.baseUrl===base&&p.provider===draft.provider)?.id||'custom';
}
export function modelIssue(draft:Draft):string {
 try {
  const url=new URL(draft.baseUrl);
  if(!['http:','https:'].includes(url.protocol)||url.username||url.password||url.search||url.hash)throw Error();
 }catch{return '请输入有效的 HTTP 或 HTTPS 地址';}
 if(!draft.model.trim())return '请填写模型 ID';
 if(!['responses','chat-completions','anthropic-messages'].includes(draft.protocol))return '请选择 API 协议';
 return '';
}
export function verifiedModels(state:MessageShape<typeof RegistryStateSchema>) {
 return state.models.filter(model=>model.profile?.model&&model.profile.baseUrl&&model.profile.verifiedAtMs>0n);
}
export function setupProgress(state:MessageShape<typeof RegistryStateSchema>) {
 const models=new Set(verifiedModels(state).map(model=>model.id));
 const agents=state.agents.filter(a=>models.has(a.modelConnectionId));
 const managed=state.devices.filter(device=>device.runtimeState==='managed');
 const deviceIds=managed.filter(device=>agents.some(agent=>agent.id===device.defaultAgentId&&agent.deviceIds.includes(device.id))).map(device=>device.id);
 return {modelReady:models.size>0,ready:managed.length>0&&deviceIds.length===managed.length,deviceIds};
}
type Frame={phoneId?:string;deviceId?:string;width:number;height:number;png:Uint8Array;observationId:string};
export function hasDeviceFrame(id:string,frame:Frame):boolean {
 return (frame.phoneId??frame.deviceId)===id&&frame.width>0&&frame.height>0&&frame.png.length>0&&!!frame.observationId;
}
