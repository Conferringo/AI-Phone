import {expect,test} from 'bun:test';
import {connectionKey,legacyAgent,managedDevice,migrateAgentScopes} from './scopes';
test('legacy bindings and pending deliveries migrate once without losing pairing or receipt state',()=>{
 const config={appId:'app',secret:'fixture',allowed:['owner']},job={provider:'qq',runId:'existing-run',stage:'unknown',textSent:true};
 const vault:{configs:Record<string,unknown>;jobs:any[]}={configs:{qq:config},jobs:[job]};
 migrateAgentScopes(vault);expect(vault.configs[connectionKey(legacyAgent,'qq')]).toBe(config);expect(vault.configs.qq).toBeUndefined();expect(vault.jobs[0]).toEqual({...job,agentId:legacyAgent});
 const snapshot=JSON.stringify(vault);migrateAgentScopes(vault);expect(JSON.stringify(vault)).toBe(snapshot);
});
test('same provider credentials remain independent for different Agents',()=>{
 const vault={configs:{[connectionKey('a','qq')]:{appId:'a-bot'},[connectionKey('b','qq')]:{appId:'b-bot'}},jobs:[]};
 delete vault.configs[connectionKey('a','qq')];expect(vault.configs[connectionKey('b','qq')]).toEqual({appId:'b-bot'});
});
test('external commands route only to installed devices bound to the calling Agent',()=>{
 const state={agents:[{id:'a',deviceIds:['pending','shared']},{id:'b',deviceIds:['other']}],devices:[{id:'pending',runtimeState:'registered',defaultAgentId:'a'},{id:'shared',runtimeState:'managed',defaultAgentId:'b'},{id:'other',runtimeState:'managed',defaultAgentId:'b'}]};
 expect(managedDevice('a',state)).toBe('shared');expect(managedDevice('b',state)).toBe('other');expect(managedDevice('unknown',state)).toBeNull();state.agents[0].deviceIds=['pending'];expect(managedDevice('a',state)).toBeNull();
});
