import {expect,test} from 'bun:test';
import {entryLanguage} from '../apps/web/src/lib/locale';
test('connection language accepts only explicit supported choices',()=>{
 for(const value of ['zh-CN','en','zh-TW','ja','ko','de','fr','es'])expect(entryLanguage(value)).toBe(value);
 for(const value of [null,'','../../en','en; Domain=example.com','de-DE','EN','https://example.com'])expect(entryLanguage(value)).toBeNull();
});
