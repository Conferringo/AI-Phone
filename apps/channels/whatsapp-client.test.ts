import {test,expect} from 'bun:test';
import {LocalAuth} from 'whatsapp-web.js';
import {WhatsAppClient} from './whatsapp-client';

test('destroy closes Puppeteer 25 without the removed isConnected method',async()=>{
 const events:string[]=[];const auth=new LocalAuth();auth.destroy=async()=>{events.push('auth')};
 const client=new WhatsAppClient({authStrategy:auth});
 Object.defineProperty(client,'pupBrowser',{value:{connected:true,close:async()=>{events.push('browser')}}});
 await client.destroy();expect(events).toEqual(['browser','auth']);
});
test('logout waits for remote logout and browser closure before deleting account state',async()=>{
 const events:string[]=[];const auth=new LocalAuth();auth.logout=async()=>{events.push('erase')};
 const client=new WhatsAppClient({authStrategy:auth});
 Object.defineProperty(client,'pupPage',{value:{evaluate:async()=>{events.push('remote')}}});
 Object.defineProperty(client,'pupBrowser',{value:{connected:true,close:async()=>{events.push('browser')}}});
 await client.logout();expect(events).toEqual(['remote','browser','erase']);
});
test('an unknown close result preserves account state and reports failure',async()=>{
 let erased=false;const auth=new LocalAuth();auth.logout=async()=>{erased=true};
 const client=new WhatsAppClient({authStrategy:auth});
 Object.defineProperty(client,'pupPage',{value:{evaluate:async()=>{}}});
 Object.defineProperty(client,'pupBrowser',{value:{close:async()=>{throw Error('close unknown')}}});
 await expect(client.logout()).rejects.toThrow('close unknown');expect(erased).toBe(false);
});
test('stopping before initialization is harmless and partial initialization cannot log out',async()=>{
 let destroyed=0,erased=0;const auth=new LocalAuth();auth.destroy=async()=>{destroyed++};auth.logout=async()=>{erased++};
 const client=new WhatsAppClient({authStrategy:auth});await client.destroy();expect(destroyed).toBe(1);
 for(const available of ['pupPage','pupBrowser']){
  const partial=new WhatsAppClient({authStrategy:auth});Object.defineProperty(partial,available,{value:{}});
  await expect(partial.logout()).rejects.toThrow('not initialized');
 }
 expect(erased).toBe(0);
});
