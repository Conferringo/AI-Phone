'use client';
import {SelectField} from './ui/select';
import {retireClientStorage} from '@/lib/client-storage';
import {createContext,useCallback,useContext,useEffect,useState} from 'react';
import {type Locale,normalizeLocale,supportedLocales,localeNames,localeDirection} from '@/lib/locale';
import english from '@/locales/en.json';
import traditional from '@/locales/zh-TW.json';
import japanese from '@/locales/ja.json';
import korean from '@/locales/ko.json';
import german from '@/locales/de.json';
import french from '@/locales/fr.json';
import spanish from '@/locales/es.json';
import russian from '@/locales/ru.json';
import arabic from '@/locales/ar.json';
const messages:Partial<Record<Locale,Record<string,string>>>={en:english,'zh-TW':traditional,ja:japanese,ko:korean,de:german,fr:french,es:spanish,ru:russian,ar:arabic};
type Values=Record<string,string|number>;
const interpolate=(value:string,values?:Values)=>value.replace(/\{(\w+)\}/g,(match,key)=>values?.[key]===undefined?match:String(values[key]));
const Context=createContext({locale:'zh-CN' as Locale,setLocale:(_value:Locale)=>{},t:(value:string,values?:Values)=>interpolate(value,values)});
export function LocaleProvider({initialLocale,children}:{initialLocale:Locale;children:React.ReactNode}) {
 const [locale,update]=useState(initialLocale);
 const setLocale=useCallback((value:Locale)=>{const next=normalizeLocale(value);update(next);document.cookie=`ai-phone-locale=${next}; Path=/; Max-Age=31536000; SameSite=Lax`;},[]);
 useEffect(()=>{document.documentElement.lang=locale;document.documentElement.dir=localeDirection(locale);},[locale]);
 useEffect(()=>{try{retireClientStorage(localStorage);}catch{}},[]);
 const t=useCallback((value:string,values?:Values)=>interpolate(locale==='zh-CN'?value:(messages[locale]?.[value]??english[value as keyof typeof english]??value),values),[locale]);
 return <Context.Provider value={{locale,setLocale,t}}>{children}</Context.Provider>;
}
export const useI18n=()=>useContext(Context);
export function LanguageSelect({className}:{className?:string}) {
 const {locale,setLocale}=useI18n();
 return <SelectField className={className||'language-select'} aria-label="Language / 语言" value={locale} onValueChange={value=>setLocale(normalizeLocale(value))} options={supportedLocales.map(value=>({value,label:localeNames[value]}))}/>;
}
