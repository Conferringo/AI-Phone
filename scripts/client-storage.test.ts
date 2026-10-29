import {expect,test} from 'bun:test';
import {retireClientStorage} from '../apps/web/src/lib/client-storage';
test('upgrade removes retired preferences while preserving conversation and language state',()=>{
 const values=new Map([['ai-phone.pet.preferences.v1','old'],['ai-phone.pet.position.v1','old'],['ai-phone:chat-display-cut:phone-local-01','keep'],['ai-phone:outbox','keep'],['language','de']]);
 const storage={removeItem:(key:string)=>{values.delete(key);}};
 retireClientStorage(storage);retireClientStorage(storage);
 expect([...values.keys()]).toEqual(['ai-phone:chat-display-cut:phone-local-01','ai-phone:outbox','language']);
});
test('restricted client storage does not prevent startup',()=>{expect(()=>retireClientStorage({removeItem(){throw Error('denied');}})).not.toThrow();});
