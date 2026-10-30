import {expect,test} from 'bun:test';
import {create} from '@bufbuild/protobuf';
import {RegistryStateSchema} from '../packages/protocol/src/gen/ai_phone/v1/registry_pb';
import {ModelDraftSchema} from '../packages/protocol/src/gen/ai_phone/v1/settings_pb';
import {modelPreset,modelPresets,presetDraft,modelIssue,setupProgress,hasDeviceFrame} from '../apps/web/src/lib/registry-setup';

test('all provider presets resolve independently with editable endpoints and no inherited key',()=>{
 expect(new Set(modelPresets.map(p=>p.id)).size).toBe(modelPresets.length);
 for(const preset of modelPresets){const draft=presetDraft(preset.id);expect(draft.apiKey).toBe('');expect(draft.useSavedKey).toBe(false);expect(modelPreset(draft)).toBe(preset.id);expect(['responses','chat-completions','anthropic-messages']).toContain(draft.protocol);}
 expect(presetDraft('anthropic').protocol).toBe('anthropic-messages');
 expect(presetDraft('minimax').protocol).toBe('anthropic-messages');
 expect(presetDraft('kimi-cn').baseUrl).not.toBe(presetDraft('kimi').baseUrl);
 expect(presetDraft('glm').baseUrl).not.toBe(presetDraft('zai').baseUrl);
 expect(presetDraft('qwen').baseUrl).not.toBe(presetDraft('qwen-global').baseUrl);
});

test('provider changes clear foreign credentials and use an explicit supported protocol',()=>{
 const deepseek=presetDraft('deepseek');
 expect(deepseek.protocol).toBe('chat-completions');
 expect(deepseek.model).toBe('deepseek-flash');
 expect(modelPreset(deepseek)).toBe('deepseek');
 expect(presetDraft('openai')).toMatchObject({provider:'compatible',protocol:'responses',baseUrl:'https://api.openai.com/v1',model:'',apiKey:'',useSavedKey:false});
 expect(presetDraft('custom').baseUrl).toBe('');
 expect(modelPreset({...deepseek,baseUrl:'https://proxy.example/v1'})).toBe('custom');
 expect(modelPreset(presetDraft('openai'))).toBe('openai');
 expect(modelPreset({...deepseek,baseUrl:'https://api.openai.com/v1'})).toBe('custom');
});
test('model validation allows local no-key services but rejects malformed destinations and incomplete models',()=>{
 const draft=create(ModelDraftSchema,{provider:'compatible',protocol:'chat-completions',baseUrl:'http://localhost:11434/v1',model:'local-vision'});
 expect(modelIssue(draft)).toBe('');
 for(const baseUrl of ['ftp://localhost','https://user:secret@example.com','https://:secret@example.com','https://example.com?key=secret','https://example.com#secret','invalid'])expect(modelIssue({...draft,baseUrl})).toBe('请输入有效的 HTTP 或 HTTPS 地址');
 expect(modelIssue({...draft,model:' '})).toBe('请填写模型 ID');
 expect(modelIssue({...draft,protocol:'unknown'})).toBe('请选择 API 协议');
});
const registry=()=>create(RegistryStateSchema,{devices:[{id:'phone',name:'Phone',kind:'phone',runtimeState:'managed',defaultAgentId:'agent'}],agents:[{id:'agent',modelConnectionId:'model',deviceIds:['phone']}],models:[{id:'model',profile:{model:'vision',baseUrl:'http://localhost/v1',verifiedAtMs:1n}}]});
test('setup uses persisted verification and explicit device bindings, never key presence or a local completion flag',()=>{
 expect(setupProgress(create(RegistryStateSchema,{models:[{id:'new'}]})).next).toBe('agents');
 const state=registry();expect(setupProgress(state).ready).toBe(true);
 state.models[0].profile!.verifiedAtMs=0n;expect(setupProgress(state).next).toBe('agents');
 state.models[0].profile!.verifiedAtMs=1n;state.agents[0].deviceIds=[];
 expect(setupProgress(state).ready).toBe(false);expect(setupProgress(state).next).toBe('devices');
 state.agents[0].deviceIds=['phone'];state.devices[0].runtimeState='registered';
 expect(setupProgress(state).ready).toBe(false);
 state.devices[0].runtimeState='managed';state.devices.push({...state.devices[0],id:'optional',runtimeState:'registered'});
 expect(setupProgress(state).deviceIds).toEqual(['phone']);
});
test('device check needs matching identity, dimensions, observation receipt and actual image bytes',()=>{
 const frame={width:720,height:1440,png:new Uint8Array([137]),observationId:'receipt'};
 expect(hasDeviceFrame('phone',{...frame,phoneId:'phone'})).toBe(true);
 expect(hasDeviceFrame('desktop',{...frame,deviceId:'desktop'})).toBe(true);
 expect(hasDeviceFrame('other',{...frame,phoneId:'phone'})).toBe(false);
 expect(hasDeviceFrame('phone',{...frame,phoneId:'phone',png:new Uint8Array()})).toBe(false);
 expect(hasDeviceFrame('phone',{...frame,phoneId:'phone',observationId:''})).toBe(false);
 expect(hasDeviceFrame('phone',{...frame,phoneId:'phone',width:0})).toBe(false);
 expect(hasDeviceFrame('phone',{...frame,phoneId:'phone',height:0})).toBe(false);
 expect(hasDeviceFrame('phone',{...frame,phoneId:'phone',width:1,height:1})).toBe(true);
 expect(hasDeviceFrame('phone',{...frame,phoneId:'',deviceId:'phone'})).toBe(false);
});
