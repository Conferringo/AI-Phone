export type FocusDevice={id:string;defaultAgentId:string};
export function focusGroups(devices:FocusDevice[],focus:string[],active:string,running:Record<string,{agentId?:string}>={}){
 return [...new Set(focus)].flatMap(id=>{
  const device=devices.find(d=>d.id===id);
  if(!device)return [];
  return [{agentId:running[id]?.agentId||device.defaultAgentId||`unassigned:${id}`,deviceIds:[id],chatDeviceId:id}];
 });
}
export function toggleFocus(ids:string[],id:string){return ids.includes(id)?ids.filter(value=>value!==id):[...ids,id];}
