import {expect,test} from 'bun:test';
import {clientId} from '../apps/web/src/lib/client-id';

test('LAN HTTP without randomUUID still generates unique cryptographic action IDs',()=>{
  const source={getRandomValues:crypto.getRandomValues.bind(crypto)};
  const ids=Array.from({length:1000},()=>clientId(source));
  expect(new Set(ids).size).toBe(1000);
  for(const id of ids)expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});

test('native UUID provider keeps its receiver and is used when present',()=>{
  const source={getRandomValues:crypto.getRandomValues.bind(crypto),randomUUID(){expect(this).toBe(source);return '00000000-0000-4000-8000-000000000001' as `${string}-${string}-${string}-${string}-${string}`;}};
  expect(clientId(source)).toBe('00000000-0000-4000-8000-000000000001');
});
