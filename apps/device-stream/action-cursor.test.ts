import {test,expect} from 'bun:test';
import {create} from '@bufbuild/protobuf';
import {DeviceActionRequestSchema} from '@ai-phone/protocol/ai_phone/v1/phone_runtime_pb';
import {ActionCursor} from './action-cursor';
const request=create(DeviceActionRequestSchema,{phoneId:'phone',actionId:'action-1',controller:'assistant',action:{case:'swipe',value:{x1:100,y1:400,x2:650,y2:900,durationMs:900}}});
test('accepted request never fabricates unacknowledged points',()=>{
 const cursor=new ActionCursor();cursor.begin(request,720,1440);expect(cursor.snapshot().inputConfirmed).toBe(false);expect(cursor.snapshot().paths).toHaveLength(0);
 cursor.confirm([{x:100,y:400,pointerId:0}]);cursor.confirm([{x:230,y:500,pointerId:0}]);cursor.finish('unknown');
 const value=cursor.snapshot();expect(value.status).toBe('unknown');expect(value.paths[0].points.at(-1)?.x).toBe(230);expect(value.paths[0].points.some(p=>p.x===650)).toBe(false);
});
test('new action clears previous confirmations and keeps independent fingers bounded',()=>{
 const cursor=new ActionCursor();cursor.begin(request,720,1440);cursor.confirm([{x:100,y:400,pointerId:0},{x:200,y:400,pointerId:1}]);expect(cursor.snapshot().paths).toHaveLength(2);
 cursor.begin({...request,actionId:'action-2'},1440,720);expect(cursor.snapshot().inputConfirmed).toBe(false);expect(cursor.snapshot().paths).toHaveLength(0);expect(cursor.snapshot().width).toBe(1440);
});
