'use client';
import {Check} from 'lucide-react';
import {supportedLocales,localeNames,localeGlyphs} from '@/lib/locale';
import {useI18n} from './locale-provider';
import {FloatingWindow,WindowTrigger} from './floating-window';
export function LanguageTrigger({open,onOpen}:{open:boolean;onOpen:()=>void}){
 const {locale}=useI18n();
 return <WindowTrigger label={localeNames[locale]} open={open} onOpen={onOpen}><span className="language-glyph" lang={locale}>{localeGlyphs[locale]}</span></WindowTrigger>;
}
export function LanguageWindow({open,onClose}:{open:boolean;onClose:()=>void}){
 const {locale,setLocale}=useI18n();
 return <FloatingWindow id="language" title="Language" open={open} onClose={onClose} width={280} height={480} fitContent><div className="language-options" role="group" aria-label="Language">{supportedLocales.map(value=><button key={value} type="button" lang={value} dir={value==='ar'?'rtl':'ltr'} aria-pressed={value===locale} onClick={()=>{setLocale(value);onClose();}}><span className="language-glyph">{localeGlyphs[value]}</span><span>{localeNames[value]}</span>{value===locale&&<Check size={16}/>}</button>)}</div></FloatingWindow>;
}
