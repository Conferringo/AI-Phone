'use client';
import {useEffect,useRef,useState} from 'react';
import {useRouter} from 'next/navigation';
import {fromBinary,toBinary,create} from '@bufbuild/protobuf';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {LoginRequestSchema} from '@ai-phone/protocol/ai_phone/v1/auth_pb';
import {LoaderCircle} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {useI18n,LanguageSelect} from './locale-provider';

/** Session setup only. Existing local owners and all device data are reused. */
export function SessionBootstrap(){
 const {t}=useI18n(),router=useRouter(),busy=useRef(false);
 const [error,setError]=useState(''),[passwordMode,setPasswordMode]=useState(false),[password,setPassword]=useState(''),[pending,setPending]=useState(true);
 async function connect(signal?:AbortSignal,credential?:string){
  if(busy.current)return;
  busy.current=true;setPending(true);setError('');
  try{
   const timeout=AbortSignal.timeout(10000);
   const result=await fetch('/api/auth/'+(credential===undefined?'enter':'login'),{method:'POST',headers:{'content-type':'application/x-protobuf'},body:credential===undefined?new Uint8Array():toBinary(LoginRequestSchema,create(LoginRequestSchema,{username:'admin',password:credential})),cache:'no-store',signal:signal?AbortSignal.any([signal,timeout]):timeout});
   if(signal?.aborted)return;
   if(!result.ok){
    const failure=fromBinary(ApiErrorSchema,new Uint8Array(await result.arrayBuffer()));
    if(failure.code==='local_entry_disabled'){setPasswordMode(true);return;}
    throw Error(failure.message);
   }
   setPassword('');router.refresh();
  }catch(e){if(!signal?.aborted)setError(e instanceof Error?e.message:t('连接失败'));}
  finally{busy.current=false;if(!signal?.aborted)setPending(false);}
 }
 useEffect(()=>{
  // Skip the discarded StrictMode effect before issuing a session request.
  const abort=new AbortController();
  void Promise.resolve().then(()=>{if(!abort.signal.aborted)void connect(abort.signal);});
  return()=>abort.abort();
  // Session setup is independent of label/language changes.
  // eslint-disable-next-line react-hooks/exhaustive-deps
 },[]);
 return <main className="entry-screen" aria-busy={pending}>
  {passwordMode?<form className="entry-card" onSubmit={e=>{e.preventDefault();void connect(undefined,password);}}>
   <header className="entry-heading"><h1>AI Phone</h1><LanguageSelect/></header>
   <label className="entry-field" htmlFor="owner-password">{t('管理员密码')}
   <input id="owner-password" type="password" autoComplete="current-password" value={password} onChange={e=>setPassword(e.target.value)} disabled={pending} required autoFocus/></label>
   {error&&<p role="alert">{error}</p>}
   <Button disabled={pending} type="submit">{pending?<LoaderCircle className="spinning" size={18}/>:t('登录')}</Button>
  </form>:error?<div className="entry-card"><p role="alert">{error}</p><Button onClick={()=>void connect()}>{t('重试')}</Button></div>:<LoaderCircle className="startup-spinner" size={24} aria-label={t('正在连接…')} role="status"/>}
 </main>;
}
