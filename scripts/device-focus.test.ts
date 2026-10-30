import {expect,test} from 'bun:test';
import {focusGroups,toggleFocus} from '../apps/web/src/lib/device-focus';
const devices=[{id:'phone',defaultAgentId:'a'},{id:'desktop',defaultAgentId:'a'},{id:'other',defaultAgentId:'b'}];
test('same Agent keeps both device conversations visible in device order',()=>{
 const panes=[{agentId:'a',deviceIds:['phone'],chatDeviceId:'phone'},{agentId:'a',deviceIds:['desktop'],chatDeviceId:'desktop'}];
 expect(focusGroups(devices,['phone','desktop'],'desktop')).toEqual(panes);
 expect(focusGroups(devices,['phone','desktop'],'phone')).toEqual(panes);
});
test('different Agents get separate conversation panes, including a task pinned to an earlier Agent',()=>{
 expect(focusGroups(devices,['phone','other'],'other').map(g=>g.chatDeviceId)).toEqual(['phone','other']);
 expect(focusGroups(devices,['phone','desktop'],'desktop',{phone:{agentId:'earlier-agent'}}).map(g=>g.agentId)).toEqual(['earlier-agent','a']);
});
test('unassigned devices do not share a conversation and stale focused IDs are ignored',()=>{
 const list=[{id:'x',defaultAgentId:''},{id:'y',defaultAgentId:''}];expect(focusGroups(list,['x','missing','y'],'x').length).toBe(2);
 const chosen=['phone'];expect(toggleFocus(chosen,'desktop')).toEqual(['phone','desktop']);expect(toggleFocus(chosen,'phone')).toEqual([]);expect(chosen).toEqual(['phone']);
});
