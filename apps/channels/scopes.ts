import {providers,type Provider} from './core';
export const legacyAgent='00000000-0000-0000-0000-000000000002';
export const connectionKey=(agentId:string,provider:Provider)=>`${agentId}/${provider}`;
export function authorizedDevice<T extends {id:string;runtimeState:string}>(agentId:string,deviceId:string,state:{agents:Array<{id:string;deviceIds:string[]}>;devices:T[]}):T|undefined {
 const agent=state.agents.find(a=>a.id===agentId);
 return agent?.deviceIds.includes(deviceId)?state.devices.find(d=>d.id===deviceId&&d.runtimeState==='managed'):undefined;
}
export function migrateAgentScopes<T extends {configs:Record<string,unknown>;jobs:Array<{agentId?:string}>}>(vault:T):T{
 for(const provider of providers){if(vault.configs[provider]){const key=connectionKey(legacyAgent,provider);if(vault.configs[key])throw new Error('ambiguous_legacy_binding');vault.configs[key]=vault.configs[provider];delete vault.configs[provider];}}
 for(const job of vault.jobs)job.agentId ||= legacyAgent;
 return vault;
}
export function managedDevice(agentId:string,state:{agents:Array<{id:string;deviceIds:string[]}>;devices:Array<{id:string;runtimeState:string;defaultAgentId:string}>}):string|null{
 const agent=state.agents.find(a=>a.id===agentId);
 const allowed=state.devices.filter(d=>d.runtimeState==='managed'&&agent?.deviceIds.includes(d.id));
 return (allowed.find(d=>d.defaultAgentId===agentId)||allowed[0])?.id??null;
}
