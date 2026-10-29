export const supportedLocales=['zh-CN','en','zh-TW','ja','ko','de','fr','es'] as const;
export type Locale=typeof supportedLocales[number];
export const localeNames:Record<Locale,string>={'zh-CN':'简体中文',en:'English','zh-TW':'繁體中文',ja:'日本語',ko:'한국어',de:'Deutsch',fr:'Français',es:'Español'};
export function normalizeLocale(value:string|null|undefined):Locale{
 const match=supportedLocales.find(locale=>locale.toLowerCase()===(value||'').toLowerCase());return match||'zh-CN';
}
export function preferredLocale(value:string|null|undefined):Locale{
 for(const entry of (value||'').split(',')){
  const language=entry.split(';')[0].trim().toLowerCase();
  if(/^zh-(tw|hk|hant)/.test(language))return 'zh-TW';if(language.startsWith('zh'))return 'zh-CN';
  const base=language.split('-')[0];if(supportedLocales.includes(base as Locale))return base as Locale;
 }
 return value?'en':'zh-CN';
}
/** An explicit native-client choice must not silently become a different language. */
export function entryLanguage(value:string|null):Locale|null{
 return supportedLocales.includes(value as Locale)?value as Locale:null;
}
