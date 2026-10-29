import {expect,test} from 'bun:test';
import {normalizeLocale,preferredLocale} from '../apps/web/src/lib/locale';
import english from '../apps/web/src/locales/en.json';
import {supportedLocales} from '../apps/web/src/lib/locale';
import {readFileSync} from 'node:fs';
test('language selection is bounded and browser preferences have a deterministic fallback',()=>{
 expect(normalizeLocale('en')).toBe('en');expect(normalizeLocale('../../secret')).toBe('zh-CN');
 expect(preferredLocale('en-US,en;q=0.9')).toBe('en');expect(preferredLocale('zh-TW,en;q=0.9')).toBe('zh-TW');
 expect(preferredLocale('')).toBe('zh-CN');
});
test('core installer, model and device controls have English text',()=>{
 for(const name of ['初始化 AI Phone','配置中心','API 协议','手机处理器','安装系统','截图','保存图片','附到对话','连接应用','发送消息'])expect((english as Record<string,string>)[name]).toMatch(/[a-z]/i);
});

test('all eight language catalogs contain the same complete control keys',()=>{
 const keys=Object.keys(english).sort();
 for(const locale of supportedLocales.filter(l=>l!=='zh-CN')){const catalog=JSON.parse(readFileSync(new URL('../apps/web/src/locales/'+locale+'.json',import.meta.url),'utf8'));expect(Object.keys(catalog).sort()).toEqual(keys);expect(Object.values(catalog).every(v=>typeof v==='string'&&v.length>0)).toBe(true);}
 expect(preferredLocale('de-DE,de;q=0.9')).toBe('de');expect(preferredLocale('ja-JP')).toBe('ja');expect(preferredLocale('zh-Hant-HK')).toBe('zh-TW');
});
