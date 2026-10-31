import {expect,test} from 'bun:test';
import {WorkbenchNoticeStore} from '../apps/web/src/lib/workbench-notices';

const notice={message:'HTTP 402',scope:'phone',label:'Phone'};
test('polls keep notice identity; dismissal survives the same failure until recovery',()=>{
 const store=new WorkbenchNoticeStore();store.set('run',notice);const first=store.snapshot();
 store.set('run',{...notice});expect(store.snapshot()).toBe(first);
 store.dismiss(first[0].id);store.set('run',{...notice});expect(store.snapshot()).toEqual([]);
 store.set('run',null);store.set('run',notice);expect(store.snapshot()).toHaveLength(1);
});
test('one error reported by run and exploration appears once, with a working retry',()=>{
 const store=new WorkbenchNoticeStore();let retries=0;
 store.set('run',notice);store.set('exploration',{...notice,retry:()=>retries++});
 expect(store.snapshot()).toHaveLength(1);store.snapshot()[0].retry?.();expect(retries).toBe(1);
 store.set('run',null);expect(store.snapshot()).toHaveLength(1);
 store.set('exploration',null);expect(store.snapshot()).toEqual([]);
});
test('independent devices retain their failures and each can be dismissed',()=>{
 const store=new WorkbenchNoticeStore();store.set('a',notice);store.set('b',{...notice,scope:'desktop',label:'Desktop'});
 expect(store.snapshot()).toHaveLength(2);store.dismiss(store.snapshot().find(x=>x.scope==='desktop')!.id);expect(store.snapshot()).toHaveLength(1);
 store.set('b',{...notice,scope:'desktop',label:'Desktop',message:'Offline'});expect(store.snapshot()).toHaveLength(2);
});
test('subscribers stop on disposal and removing one source cannot erase another',()=>{
 const store=new WorkbenchNoticeStore();let count=0;const unsubscribe=store.subscribe(()=>count++);
 store.set('a',notice);store.set('b',{...notice,message:'Offline'});store.set('a',null);
 expect(store.snapshot().map(x=>x.message)).toEqual(['Offline']);unsubscribe();store.set('b',null);expect(count).toBe(3);
});
