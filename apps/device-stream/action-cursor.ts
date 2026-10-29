import {create,type MessageShape} from '@bufbuild/protobuf';
import {DeviceCursorSchema,DeviceActionRequestSchema} from '@ai-phone/protocol/ai_phone/v1/phone_runtime_pb';
type Contact={x:number;y:number;pointerId:number};
export class ActionCursor {
 private value=create(DeviceCursorSchema);private began=0;
 begin(request:MessageShape<typeof DeviceActionRequestSchema>,width:number,height:number){
  const action=request.action;
  this.began=performance.now();
  this.value=create(DeviceCursorSchema,{deviceId:request.phoneId,actionId:request.actionId,actor:request.controller,kind:action.case==='longPress'?'hold':action.case||'',status:'running',startedAtMs:BigInt(Date.now()),width,height});
 }
 // Called only after the input API acknowledges this batch; no future points are stored.
 confirm(contacts:Contact[]){
  const atMs=Math.round(performance.now()-this.began);
  for(const contact of contacts){
   if(!Number.isFinite(contact.x)||!Number.isFinite(contact.y))continue;
   let path=this.value.paths.find(p=>p.pointerId===contact.pointerId);
   if(!path){path={pointerId:contact.pointerId,points:[],$typeName:'ai_phone.v1.FingerPath'};this.value.paths.push(path);}
   const last=path.points.at(-1),point={x:Math.round(contact.x),y:Math.round(contact.y),atMs,$typeName:'ai_phone.v1.GesturePoint' as const};
   if(last?.atMs===atMs)path.points[path.points.length-1]=point;else path.points.push(point);
   if(path.points.length>128)path.points.splice(1,1);
  }
  this.value.inputConfirmed=this.value.paths.length>0;this.value.durationMs=atMs;
 }
 finish(status:'completed'|'unknown'){this.value.status=status;}
 snapshot(){return create(DeviceCursorSchema,{...this.value,reportedAtMs:BigInt(Date.now())});}
}
