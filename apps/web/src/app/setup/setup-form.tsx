'use client';
import {useI18n,LanguageSelect} from '@/components/locale-provider';
import {useEffect,useState} from 'react';
import {ArrowRight} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {settingsRpc} from '@/lib/settings-client';
import {SettingsRequestSchema,SetupStateSchema,SetupOwnerRequestSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
export default function Setup(){
 const {t}=useI18n();

  const [code,setCode]=useState(''),[password,setPassword]=useState(''),[repeat,setRepeat]=useState(''),[error,setError]=useState(''),[ready,setReady]=useState(false),[busy,setBusy]=useState(false);
  async function check(){setError('');try{const result=await settingsRpc('status',SettingsRequestSchema,{},SetupStateSchema,'setup');if(result.ownerCreated)location.replace('/phone');else setReady(true);}catch(e){setError(e instanceof Error?e.message:t("读取失败"));}}
  useEffect(()=>{void check();},[]);
  async function submit(e:React.FormEvent){e.preventDefault();if(busy)return;if(password!==repeat){setError(t("两次密码不一致"));return;}setBusy(true);setError('');try{await settingsRpc('owner',SetupOwnerRequestSchema,{setupCode:code,password},SetupStateSchema,'setup');location.replace('/phone');}catch(e){setError(e instanceof Error?e.message:t("创建失败"));setBusy(false);}}
  return <main className="entry-screen"><form className="entry-card" aria-label={t("初始化管理员")} onSubmit={submit}>
   <header className="entry-heading"><h1>{t("初始化 AI Phone")}</h1><LanguageSelect/></header>
   {ready?<><div className="entry-fields">
    <label className="entry-field" htmlFor="setup-code">{t("本机初始化码")}<input id="setup-code" value={code} onChange={e=>setCode(e.target.value)} autoComplete="off" spellCheck={false} required disabled={busy} aria-describedby="setup-code-hint"/></label>
    <p id="setup-code-hint" className="field-hint">{t("从安装目录的 data/assistant/setup-code 读取。")}</p>
    <label className="entry-field" htmlFor="setup-password">{t("管理员密码")}<input id="setup-password" type="password" autoComplete="new-password" minLength={8} maxLength={128} value={password} onChange={e=>setPassword(e.target.value)} required disabled={busy}/></label>
    <label className="entry-field" htmlFor="setup-repeat">{t("确认密码")}<input id="setup-repeat" type="password" autoComplete="new-password" value={repeat} onChange={e=>setRepeat(e.target.value)} required disabled={busy}/></label>
   </div>{error&&<p role="alert">{error}</p>}<Button type="submit" disabled={busy}>{busy?t("正在创建…"):t("创建管理员")}<ArrowRight size={18}/></Button></>:error?<><p role="alert">{error}</p><Button type="button" onClick={()=>void check()}>{t("重试")}</Button></>:<p role="status">{t("正在读取初始化状态…")}</p>}
  </form></main>;
}
