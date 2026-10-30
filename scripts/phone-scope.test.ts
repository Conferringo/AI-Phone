import {test,expect} from 'bun:test';
import {create,toBinary,fromBinary} from '@bufbuild/protobuf';
import {PhoneScopeSchema,DeviceRequestSchema,TakeControlRequestSchema,DeviceActionRequestSchema} from '../packages/protocol/src/gen/ai_phone/v1/phone_runtime_pb';
test('route observe, control and action without interpreting incompatible tags',()=>{
 const phoneId='second-phone';
 const messages=[toBinary(DeviceRequestSchema,create(DeviceRequestSchema,{phoneId,includeElements:true})),toBinary(TakeControlRequestSchema,create(TakeControlRequestSchema,{phoneId,controller:'assistant'})),toBinary(DeviceActionRequestSchema,create(DeviceActionRequestSchema,{phoneId,runId:'run-1',controlEpoch:5n,action:{case:'tap',value:{x:40,y:90}}}))];
 for(const bytes of messages)expect(fromBinary(PhoneScopeSchema,bytes).phoneId).toBe(phoneId);
});
