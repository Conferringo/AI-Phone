import {expect,test} from 'bun:test';
import {focusGroups} from '../apps/web/src/lib/device-focus';
const devices=[{id:'phone',defaultAgentId:'a'},{id:'desktop',defaultAgentId:'a'},{id:'other',defaultAgentId:'b'}];
test('same Agent keeps both device conversations visible in device order',()=>{
 const panes=[{agentId:'a',deviceIds:['phone'],chatDeviceId:'phone'},{agentId:'a',deviceIds:['desktop'],chatDeviceId:'desktop'}];
 expect(focusGroups(devices,['phone','desktop'])).toEqual(panes);
});
test('different Agents get separate conversation panes, including a task pinned to an earlier Agent',()=>{
 expect(focusGroups(devices,['phone','other']).map(g=>g.chatDeviceId)).toEqual(['phone','other']);
 expect(focusGroups(devices,['phone','desktop'],{phone:{agentId:'earlier-agent'}}).map(g=>g.agentId)).toEqual(['earlier-agent','a']);
});
test('unassigned devices do not share a conversation and stale focused IDs are ignored',()=>{
 const list=[{id:'x',defaultAgentId:''},{id:'y',defaultAgentId:''}];expect(focusGroups(list,['x','missing','y']).length).toBe(2);
});
