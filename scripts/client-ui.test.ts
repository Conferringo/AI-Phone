import {test,expect} from 'bun:test';
import {connectionLocale,connectionLanguages,connectionLabels} from '../apps/client/ui/connection-i18n.js';
test('native connection language follows supported browser locales with a stable fallback',()=>{
 expect(connectionLocale('zh-Hant-HK')).toBe('zh-TW');
 expect(connectionLocale('zh-CN')).toBe('zh-CN');
 expect(connectionLocale('de-DE')).toBe('de');
 expect(connectionLocale('ja-JP')).toBe('ja');
 expect(connectionLocale('unsupported')).toBe('en');
});
test('all native connection languages cover labels and actionable errors',()=>{
 expect(connectionLanguages).toHaveLength(8);
 for(const locale of connectionLanguages){expect(connectionLabels[locale]).toHaveLength(9);expect(connectionLabels[locale].every((text:string)=>text.trim())).toBe(true);}
});
