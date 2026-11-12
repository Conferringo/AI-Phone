'use client';
import { LoadingOrb } from "@/components/ui/loading-orb";
import {create,type MessageShape} from '@bufbuild/protobuf';
import {useEffect,useRef,useState} from 'react';
import {Check,Eye,EyeOff} from 'lucide-react';
import {VoiceSettingsRequestSchema,VoiceSettingsSchema,VoiceSettingsSaveRequestSchema,VoiceCredentialRequestSchema,VoiceCredentialSchema,VoiceDraftSchema,VoiceInputProtocol} from '@ai-phone/protocol/ai_phone/v1/voice_pb';
import {settingsRpc} from '@/lib/settings-client';
import {SelectField} from './ui/select';
import {Button} from './ui/button';
import {useI18n} from './locale-provider';
import {useWorkbenchNotice} from './workbench-notices';
import {useCopilot} from './copilot';

type Draft=MessageShape<typeof VoiceDraftSchema>;
function empty(){return create(VoiceDraftSchema,{inputProtocol:VoiceInputProtocol.AUDIO_CHAT});}
export function AgentVoice({agentId,open}:{agentId:string;open:boolean}){
  const {t}=useI18n(),copilot=useCopilot();
  const [draft,setDraft]=useState<Draft>(empty),[revision,setRevision]=useState(0n),[busy,setBusy]=useState(false),[loaded,setLoaded]=useState(false),[dirty,setDirty]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState(false),[visible,setVisible]=useState(true);
  const generation=useRef(0),pending=useRef(false);
  useWorkbenchNotice(error);
  useEffect(()=>{
    if(!open||dirty)return;
    const version=++generation.current,control=new AbortController();setBusy(true);setError('');
    void (async()=>{
      try{
        const profile=await settingsRpc('settings',VoiceSettingsRequestSchema,{agentId},VoiceSettingsSchema,'voice',control.signal);
        const key=profile.hasApiKey?await settingsRpc('credential',VoiceCredentialRequestSchema,{agentId,expectedRevision:profile.revision},VoiceCredentialSchema,'voice',control.signal):null;
        if(version!==generation.current)return;
        setRevision(profile.revision);setDraft(profile.configured?create(VoiceDraftSchema,{baseUrl:profile.baseUrl,apiKey:key?.apiKey||'',recognitionModel:profile.recognitionModel,speechModel:profile.speechModel,voice:profile.voice,proxyUrl:profile.proxyUrl,inputProtocol:profile.inputProtocol}):empty());setLoaded(true);
      }catch(e){if(!control.signal.aborted)setError(e instanceof Error?e.message:t('读取失败'));}
      finally{if(version===generation.current)setBusy(false);}
    })();
    return()=>{generation.current++;control.abort();};
  },[agentId,open,dirty]);
  function edit(patch:Partial<Draft>){setDraft(v=>({...v,...patch}));setDirty(true);setSaved(false);setError('');}
  async function save(clear=false){
    if(pending.current)return;pending.current=true;setBusy(true);setError('');setSaved(false);
    try{
      const profile=await settingsRpc('save',VoiceSettingsSaveRequestSchema,{agentId,expectedRevision:revision,draft:clear?create(VoiceDraftSchema):draft},VoiceSettingsSchema,'voice');
      setRevision(profile.revision);setDirty(false);setSaved(true);if(clear)setDraft(empty());copilot.profileChanged(profile);
    }catch(e){setError(e instanceof Error?e.message:t('保存失败'));}finally{pending.current=false;setBusy(false);}
  }
  return <form className="registry-form agent-voice" onSubmit={e=>{e.preventDefault();void save();}}>
    <fieldset className="agent-voice-fields" disabled={busy||!loaded}>
      <label>{t('API 地址')}<input dir="ltr" type="url" required value={draft.baseUrl} autoCapitalize="none" spellCheck={false} placeholder="https://…/v1" onChange={e=>edit({baseUrl:e.target.value})}/></label>
      <label>API Key<span className="registry-key"><input dir="ltr" type={visible?'text':'password'} value={draft.apiKey} autoComplete="off" autoCapitalize="none" spellCheck={false} onChange={e=>edit({apiKey:e.target.value})}/><button type="button" aria-label={t(visible?'隐藏密钥':'显示密钥')} onClick={()=>setVisible(v=>!v)}>{visible?<EyeOff size={16}/>:<Eye size={16}/>}</button></span></label>
      <label>{t('输入协议')}<SelectField aria-label={t('输入协议')} value={String(draft.inputProtocol)} onValueChange={value=>edit({inputProtocol:Number(value) as VoiceInputProtocol})} options={[{value:String(VoiceInputProtocol.REALTIME_TRANSCRIPTION),label:t('实时转写')},{value:String(VoiceInputProtocol.AUDIO_CHAT),label:t('音频对话')},{value:String(VoiceInputProtocol.TRANSCRIPTION),label:t('语音转写')}]}/></label>
      <label>{t('识别模型')}<input dir="ltr" required value={draft.recognitionModel} spellCheck={false} onChange={e=>edit({recognitionModel:e.target.value})}/></label>
      <details className="registry-advanced">
        <summary>{t('高级')}</summary>
        <div className="agent-voice-row">
          <label>{t('远程合成模型')}<input dir="ltr" value={draft.speechModel} spellCheck={false} onChange={e=>edit({speechModel:e.target.value})}/></label>
          <label>{t('远程音色')}<input value={draft.voice} spellCheck={false} onChange={e=>edit({voice:e.target.value})}/></label>
        </div>
        <label>{t('代理地址')}<input dir="ltr" type="url" value={draft.proxyUrl} placeholder={t('直连')} onChange={e=>edit({proxyUrl:e.target.value})}/></label>
      </details>
    </fieldset>
    <footer className="agent-voice-actions"><Button type="submit" disabled={busy||!loaded||!dirty}>{busy&&<LoadingOrb size={15} />}{t('保存')}</Button>{dirty&&<Button type="button" variant="ghost" disabled={busy} onClick={()=>{setDirty(false);setError('');}}>{t('撤销')}</Button>}{revision>0n&&!!draft.recognitionModel&&<Button type="button" variant="ghost" disabled={busy} onClick={()=>void save(true)}>{t('移除配置')}</Button>}{saved&&<span role="status"><Check size={14}/>{t('已保存')}</span>}</footer>
  </form>;
}
