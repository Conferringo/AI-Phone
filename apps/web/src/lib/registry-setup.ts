import presets from '../../../../config/model-providers.json';
import {create,type MessageShape} from '@bufbuild/protobuf';
import {ModelDraftSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
import {RegistryStateSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';

type Draft=MessageShape<typeof ModelDraftSchema>;
export type ModelPreset=string;
export type SetupStep='agents'|'devices'|'review';
export const modelPresets=presets;

// Explicit selection creates a fresh connection; existing secrets/profiles are never overwritten.
export function presetDraft(preset:ModelPreset):Draft {
 const p=presets.find(p=>p.id===preset)||presets.find(p=>p.id==='custom')!;
 return create(ModelDraftSchema,{provider:p.provider,protocol:p.protocol,baseUrl:p.baseUrl,model:p.model});
}
export function modelPreset(draft:Draft):ModelPreset {
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
export function setupProgress(state:MessageShape<typeof RegistryStateSchema>) {
 const models=new Set(state.models.filter(m=>m.profile?.model&&m.profile.baseUrl&&m.profile.verifiedAtMs>0n).map(m=>m.id));
 const agents=state.agents.filter(a=>models.has(a.modelConnectionId));
 const devices=state.devices.filter(d=>d.runtimeState==='managed'&&agents.some(a=>a.id===d.defaultAgentId&&a.deviceIds.includes(d.id)));
 const modelReady=agents.length>0,ready=devices.length>0;
 return {modelReady,ready,deviceIds:devices.map(d=>d.id),next:(!modelReady?'agents':ready?'review':'devices') as SetupStep};
}
type Frame={phoneId?:string;deviceId?:string;width:number;height:number;png:Uint8Array;observationId:string};
export function hasDeviceFrame(id:string,frame:Frame):boolean {
 return (frame.phoneId??frame.deviceId)===id&&frame.width>0&&frame.height>0&&frame.png.length>0&&!!frame.observationId;
}
