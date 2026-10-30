import {test,expect} from 'bun:test';
import {connectionLocale,connectionLanguages,connectionLabels} from '../apps/client/ui/connection-i18n.js';
import {setupLabels} from '../apps/client/ui/setup-i18n.js';
test('native connection language follows supported browser locales with a stable fallback',()=>{
 expect(connectionLocale('zh-Hant-HK')).toBe('zh-TW');
 expect(connectionLocale('zh-CN')).toBe('zh-CN');
 expect(connectionLocale('de-DE')).toBe('de');
 expect(connectionLocale('ja-JP')).toBe('ja');
 expect(connectionLocale('unsupported')).toBe('en');
});
test('native installation labels and recovery stages cover every supported language',()=>{
 const keys=Object.keys(setupLabels.en).sort();
 for(const locale of connectionLanguages){expect(Object.keys(setupLabels[locale]).sort()).toEqual(keys);expect(Object.values(setupLabels[locale]).every((text)=>typeof text==='string'&&text.trim())).toBe(true);}
});
test('all native connection languages cover labels and actionable errors',()=>{
 expect(connectionLanguages).toHaveLength(10);
 for(const locale of connectionLanguages){expect(connectionLabels[locale]).toHaveLength(9);expect(connectionLabels[locale].every((text:string)=>text.trim())).toBe(true);}
});
