import {create,type MessageShape} from '@bufbuild/protobuf';
import {VoiceInputProtocol,VoiceSettingsRequestSchema,VoiceSettingsSchema,VoiceTranscribeRequestSchema,VoiceTranscriptSchema} from '@ai-phone/protocol/ai_phone/v1/voice_pb';
import {settingsRpc} from './settings-client';
import {CopilotInput,voiceOutboxInput,type VoiceDraft} from './copilot-input';
import {SpeechSegments,wav} from './copilot-audio';
import {Microphone} from './copilot-capture';
import {speechFrames,speechSocket} from './copilot-speech';
import {CopilotOutput,type SpokenReply} from './copilot-output';
import {CopilotStream,voiceSocket} from './copilot-stream';
import {LocalRecognition,installLocalRecognition,localRecognitionAvailability,supportsLocalRecognition} from './copilot-local-recognition';
import {localVoice,speakLocally} from './copilot-local-speech';
import {hasNativeSpeech,NativeRecognition} from './copilot-native';
import {VoiceSpeechRequestSchema,VoiceStreamStartSchema} from '@ai-phone/protocol/ai_phone/v1/voice_pb';
import type {OutboxInput} from './chat-outbox';
import {normalizeLocale,type Locale} from './locale';

type Profile=MessageShape<typeof VoiceSettingsSchema>;
export type CopilotBinding={deviceId:string;conversationId:string;agentId:string;ready:boolean;submit:(input:OutboxInput)=>void;recover:(text:string)=>void;replies:SpokenReply[]};
type Session={binding:CopilotBinding;core:CopilotInput};
type Target={session:Session;profile:Profile};
type AudioJob={id:string;target:Target;bytes:Uint8Array;final:boolean};
export type MicrophonePort=Pick<Microphone,'prepare'|'open'|'play'|'close'>;
type Activity='listening'|'hearing'|'speaking';
export type CopilotState={enabled:boolean;opening:boolean;error:string;currentDeviceId:string;activity:Activity;localLanguagePack:string;drafts:Record<string,VoiceDraft|undefined>};
const speechLanguages:Record<Locale,string>={'zh-CN':'zh-CN','zh-TW':'zh-TW',en:'en-US',ja:'ja-JP',ko:'ko-KR',de:'de-DE',fr:'fr-FR',es:'es-ES',ru:'ru-RU',ar:'ar-SA'};

export class CopilotSession {
  private bindings=new Map<string,CopilotBinding>();
  private profiles=new Map<string,Profile>();
  private sessions=new Map<string,Session>();
  private streams=new Map<string,CopilotStream>();
  private jobs:AudioJob[]=[];
  private working=0;
  private transport:AbortController|null=null;
  private microphone:MicrophonePort|null=null;
  private segments:SpeechSegments<Target>|null=null;
  private recognition:LocalRecognition|null=null;
  private native:NativeRecognition|null=null;
  private language='';
  private localInput=false;
  private localLanguagePack='';
  private activity:Activity='listening';
  private enabled=false;
  private opening=false;
  private error='';
  private outputError='';
  private currentDeviceId='';
  private captureId='';
  private generation=0;
  private endpointTimer:ReturnType<typeof setTimeout>|null=null;
  private listeners=new Set<()=>void>();
  private state:CopilotState={enabled:false,opening:false,error:'',currentDeviceId:'',activity:'listening',localLanguagePack:'',drafts:{}};
  private speaker=new CopilotOutput<Session>(async(session,text,signal)=>{
    this.setActivity('speaking');
    try{
      const voice=localVoice(this.language);
      if(voice){await speakLocally(text,voice,signal);return;}
      const b=session.binding,p=this.profiles.get(b.agentId);
      if(this.localInput||!p?.speechModel||!p.voice)throw Error('当前语言没有本机播报音色');
      if(!this.microphone)return;
      const request=create(VoiceSpeechRequestSchema,{deviceId:b.deviceId,agentId:b.agentId,configurationRevision:p.revision,text});
      await this.microphone.play(control=>speechFrames(request,control,this.openSpeech),signal);
    }finally{if(this.activity==='speaking')this.setActivity('listening');}
  },message=>{this.outputError=message;this.publish();});
  constructor(private configure:(agentId:string)=>void,private createMicrophone:()=>MicrophonePort=()=>new Microphone(),private openStream:()=>WebSocket=voiceSocket,private openSpeech:()=>WebSocket=speechSocket){}
  subscribe=(callback:()=>void)=>{this.listeners.add(callback);return()=>{this.listeners.delete(callback);};};
  snapshot=()=>this.state;
  private publish(){
    const drafts:CopilotState['drafts']={};
    for(const session of this.sessions.values())drafts[session.binding.deviceId]=session.core.drafts.findLast(d=>d.status!=='accepted');
    this.state={enabled:this.enabled,opening:this.opening,error:this.error||this.outputError,currentDeviceId:this.currentDeviceId,activity:this.activity,localLanguagePack:this.localLanguagePack,drafts};
    for(const listener of this.listeners)listener();
  }
  private setActivity(activity:Activity){if(this.activity!==activity){this.activity=activity;this.publish();}}
  private feed(binding:CopilotBinding){
    if(binding.ready&&(this.localInput||this.profiles.get(binding.agentId)?.configured)){
      this.speaker.feed(this.key(binding),this.session(binding),binding.replies);
    }
  }
  select(deviceId:string){if(this.currentDeviceId!==deviceId){this.currentDeviceId=deviceId;this.publish();}}
  register(binding:CopilotBinding){
    this.bindings.set(binding.deviceId,binding);
    for(const session of this.sessions.values())if(session.binding.deviceId===binding.deviceId&&session.binding.conversationId===binding.conversationId&&session.binding.agentId===binding.agentId)session.binding=binding;
    if(this.enabled)this.feed(binding);
  }
  unregister(deviceId:string){this.bindings.delete(deviceId);}
  profileChanged(profile:Profile){this.profiles.set(profile.agentId,profile);}
  private key(binding:CopilotBinding){return `${binding.deviceId}:${binding.conversationId}:${binding.agentId}`;}
  private session(binding:CopilotBinding):Session {
    const key=this.key(binding);let session=this.sessions.get(key);
    if(!session){
      const core=new CopilotInput({deviceId:binding.deviceId,conversationId:binding.conversationId,sessionId:this.captureId},commit=>{
        const owner=this.sessions.get(key);if(!owner)throw Error('语音会话已关闭');
        owner.binding.submit(voiceOutboxInput(commit));
        performance.mark('copilot.outbox.'+commit.utteranceId,{detail:{requestId:commit.requestId}});
        this.outputError='';this.speaker.release();
      });
      session={binding,core};
      this.sessions.set(key,session);
    }
    return session;
  }
  private focusedBinding():CopilotBinding {
    const binding=this.bindings.get(this.currentDeviceId);
    if(!binding?.ready)throw Error('当前设备对话正在连接');
    return binding;
  }
  private choose():Target {
    const binding=this.focusedBinding();
    const profile=this.profiles.get(binding.agentId);
    if(!profile?.configured){this.configure(binding.agentId);throw Error('请先配置 Agent 的语音服务');}
    return {session:this.session(binding),profile};
  }
  private liveStream(target:Target){
    const {binding,core}=target.session,key=this.key(binding)+':'+target.profile.revision;
    let stream=this.streams.get(key);
    if(!stream){
      stream=new CopilotStream(create(VoiceStreamStartSchema,{deviceId:binding.deviceId,agentId:binding.agentId,configurationRevision:target.profile.revision,sessionId:this.captureId}),patch=>{
        try{core.transcript({utteranceId:patch.utteranceId,revision:patch.revision,literal:patch.text,final:patch.final});if(core.error)throw Error(core.error);this.publish();}
        catch(e){this.pause(e instanceof Error?e.message:'语音识别失败');}
      },message=>this.pause(message),this.openStream);
      this.streams.set(key,stream);
    }
    return stream;
  }
  private events(){return new SpeechSegments<Target>({
    choose:()=>this.choose(),
    start:(id,target,at)=>{this.speaker.hold();this.activity='hearing';target.session.core.speech(id,at);this.publish();},
    speech:(id,target,at)=>{target.session.core.speech(id,at);},
    chunk:(id,target,frames,final)=>{this.jobs.push({id,target,bytes:wav(frames),final});if(this.jobs.length>4)this.pause('语音识别积压，录音已暂停');else void this.pump();},
    stream:(id,target,frames,final)=>{if(!this.error)this.liveStream(target).audio(id,frames,final);},
    end:(id,target,at)=>{
      this.activity='listening';
      if(target.profile.inputProtocol!==3){
        const job=this.jobs.findLast(j=>j.id===id);
        if(job)job.final=true;
        else {const draft=target.session.core.drafts.find(d=>d.utteranceId===id);if(draft?.text)target.session.core.transcript({utteranceId:id,revision:draft.revision+1,literal:draft.literal,final:true,correction:{baseRevision:draft.revision,text:draft.text}});}
      }
      target.session.core.advance(at);this.scheduleEndpoint();this.publish();
    },
  },()=>crypto.randomUUID(),target=>target.profile.inputProtocol===1,target=>target.profile.inputProtocol===3);}
  private async loadProfiles(){
    const agents=[...new Set([...this.bindings.values()].map(b=>b.agentId).filter(Boolean))];
    await Promise.all(agents.map(async agentId=>{this.profiles.set(agentId,await settingsRpc('settings',VoiceSettingsRequestSchema,{agentId},VoiceSettingsSchema,'voice'));}));
  }
  private async listenNative(generation:number){
    const microphone=this.createMicrophone();this.microphone=microphone;
    const prepared=microphone.prepare();
    const native=new NativeRecognition();this.native=native;
    const turns=new Map<string,Session>();
    await Promise.all([prepared,native.open(this.language,event=>{
      if(this.native!==native)return;
      if(event.type==='error'){this.pause(event.message);return;}
      try{
        const id=event.utteranceId;
        if(event.type==='speechstart'){
          const session=this.session(this.focusedBinding());
          turns.set(id,session);
          session.core.speech(id,event.at);
          this.speaker.hold();this.setActivity('hearing');
          performance.mark('copilot.speech-start.'+id);
          return;
        }
        const session=turns.get(id);
        if(!session)return;
        if(event.type==='speechend'){
          performance.mark('copilot.pause.'+id,{startTime:event.lastSpeechAt});
          performance.mark('copilot.speech-end.'+id);
          session.core.finish(id);
          this.setActivity('listening');
        }else{
          if(event.final&&!event.text.trim()){
            const text=session.core.recoverIncomplete(id);if(text)session.binding.recover(text);
            turns.delete(id);this.speaker.release();
          }else{
            if(event.final)performance.mark('copilot.final-text.'+id);
            session.core.transcript({utteranceId:id,revision:event.revision,literal:event.text,final:event.final});
            if(event.final){
              turns.delete(id);
            }
            if(session.core.error)throw Error(session.core.error);
          }
          this.publish();
        }
      }catch(e){this.pause(e instanceof Error?e.message:'本机语音识别失败');}
    })]);
    if(generation!==this.generation)return;
    await microphone.open((pcm,at)=>{try{native.push(pcm,at);}catch(e){this.pause(e instanceof Error?e.message:'语音输入失败');}},message=>this.pause(message));
  }
  private async listenLocally(){
    const recognition=new LocalRecognition();
    const generation=this.generation;
    this.recognition=recognition;
    let turn:{session:Session;id:string;revision:number}|undefined;
    let committed=false;
    await recognition.open(this.language,event=>{
      if(this.recognition!==recognition)return;
      try{
        if(event.type==='error'){this.pause(event.message);return;}
        if(event.type==='end'){
          if(turn){
            const text=turn.session.core.recoverIncomplete();
            if(text)turn.session.binding.recover(text);
            this.activity='listening';this.speaker.release();this.publish();
          }
          void this.listenLocally().catch(error=>{
            if(generation===this.generation)this.pause(error instanceof Error?error.message:'本机语音识别失败');
          });
          return;
        }
        if(committed||(event.type==='transcript'&&!event.text.trim()))return;
        if(!turn){
          const session=this.session(this.focusedBinding()),id=crypto.randomUUID();
          session.core.speech(id,event.at);
          turn={session,id,revision:0};
          this.speaker.hold();this.setActivity('hearing');
        }
        if(event.type==='transcript'){
          const {session,id}=turn;
          session.core.transcript({utteranceId:id,revision:++turn.revision,literal:event.text,final:event.final});
          if(event.final){
            turn=undefined;
            committed=true;
            this.activity='listening';
            session.core.finish(id);
            if(session.core.error)throw Error(session.core.error);
          }
          this.publish();
        }
      }catch(error){this.pause(error instanceof Error?error.message:'本机语音识别失败');}
    });
  }
  private async startRemote(generation:number,profilesLoaded:boolean){
    const microphone=this.createMicrophone();this.microphone=microphone;
    const gesture=microphone.prepare();
    await Promise.all([gesture,profilesLoaded?undefined:this.loadProfiles()]);
    if(generation!==this.generation)return;
    const target=this.choose();if(target.profile.inputProtocol===3)await this.liveStream(target).ready();
    if(generation!==this.generation)return;
    this.segments=this.events();
    await microphone.open((pcm,at)=>{try{this.segments?.push(pcm,at);}catch(e){this.pause(e instanceof Error?e.message:'语音输入失败');}},message=>this.pause(message));
  }
  async start(deviceId=this.currentDeviceId){
    if(this.opening)return;
    this.stop();this.select(deviceId);this.opening=true;this.error='';this.captureId=crypto.randomUUID();
    const generation=++this.generation;this.publish();
    try{
      this.focusedBinding();
      this.language=speechLanguages[normalizeLocale(document.documentElement.lang)];
      globalThis.speechSynthesis?.getVoices();
      if(hasNativeSpeech()){
        this.localInput=true;
        await this.listenNative(generation);
      }else if(supportsLocalRecognition()){
        const availability=await localRecognitionAvailability(this.language);
        if(generation!==this.generation)return;
        if(availability==='available'){
          this.localInput=true;
          await this.listenLocally();
        }else{
          await this.loadProfiles();
          if(generation!==this.generation)return;
          if(!this.profiles.get(this.focusedBinding().agentId)?.configured&&(availability==='downloadable'||availability==='downloading')){
            this.localLanguagePack=this.language;
            throw Error('本机语音需要语言包');
          }
          await this.startRemote(generation,true);
        }
      }else await this.startRemote(generation,false);
      if(generation!==this.generation)return;
      this.enabled=true;
      for(const binding of this.bindings.values())this.feed(binding);
      this.speaker.release();
    }catch(e){if(generation===this.generation)this.pause(e instanceof Error?e.message:'麦克风开启失败');}
    finally{if(generation===this.generation){this.opening=false;this.publish();}}
  }
  stop(){
    if(this.endpointTimer)clearTimeout(this.endpointTimer);this.endpointTimer=null;
    this.generation++;this.enabled=false;this.opening=false;this.transport?.abort();this.transport=null;this.speaker.close();this.microphone?.close();this.microphone=null;this.segments=null;this.jobs=[];
    this.recognition?.cancel();this.recognition=null;this.localInput=false;this.localLanguagePack='';this.activity='listening';
    this.closeNative();
    for(const stream of this.streams.values())stream.close();this.streams.clear();
    for(const session of this.sessions.values()){
      const text=session.core.drafts.filter(d=>d.status!=='accepted').map(d=>d.text).filter(Boolean).join('\n');
      session.core.stop();if(text)session.binding.recover(text);
    }
    this.sessions.clear();this.error='';this.outputError='';this.publish();
  }
  private scheduleEndpoint(){
    if(this.endpointTimer)clearTimeout(this.endpointTimer);
    const deadlines=[...this.sessions.values()].map(s=>s.core.deadline).filter((t):t is number=>t!==null);
    this.endpointTimer=deadlines.length?setTimeout(()=>{this.endpointTimer=null;for(const s of this.sessions.values())s.core.advance(performance.now());this.publish();},Math.max(0,Math.min(...deadlines)-performance.now())):null;
  }
  private closeNative(){
    const native=this.native;this.native=null;
    const generation=this.generation;
    if(native)void native.close().catch(error=>{
      if(generation===this.generation){this.error=error instanceof Error?error.message:String(error);this.publish();}
    });
  }
  private pause(message:string){const segments=this.segments;this.segments=null;this.error=message;segments?.finish(performance.now());for(const stream of this.streams.values())stream.close();this.streams.clear();this.microphone?.close();this.recognition?.cancel();this.recognition=null;this.closeNative();this.activity='listening';this.speaker.hold();this.publish();}
  retry=async()=>{
    if(this.opening)return;
    if(this.localLanguagePack){
      const generation=this.generation;
      this.opening=true;this.publish();
      try{
        if(!await installLocalRecognition(this.localLanguagePack))throw Error('本机语音语言包安装失败');
        if(generation===this.generation){this.opening=false;await this.start();}
      }catch(e){if(generation===this.generation)this.pause(e instanceof Error?e.message:'本机语音语言包安装失败');}
      finally{if(generation===this.generation){this.opening=false;this.publish();}}
      return;
    }
    if(!this.enabled){await this.start(this.currentDeviceId);return;}
    if(!this.error){this.outputError='';this.speaker.release();this.publish();return;}
    if(this.localInput){
      this.opening=true;this.error='';this.publish();
      const generation=this.generation;
      try{
        for(const session of this.sessions.values()){
          const text=session.core.recoverIncomplete();if(text)session.binding.recover(text);
          session.core.retry();if(session.core.error)throw Error(session.core.error);
        }
        if(hasNativeSpeech())await this.listenNative(generation);else await this.listenLocally();
        if(generation===this.generation)this.speaker.release();
      }catch(e){if(generation===this.generation)this.pause(e instanceof Error?e.message:'本机语音识别失败');}
      finally{if(generation===this.generation){this.opening=false;this.publish();}}
      return;
    }
    this.error='';this.opening=true;const generation=this.generation,microphone=this.createMicrophone();this.microphone=microphone;this.publish();
    try{
      const gesture=microphone.prepare();await Promise.all([gesture,this.loadProfiles()]);if(generation!==this.generation)return;
      for(const job of this.jobs)job.target.profile=this.profiles.get(job.target.session.binding.agentId)||job.target.profile;
      for(const session of this.sessions.values()){
        if(this.profiles.get(session.binding.agentId)?.inputProtocol===3){const text=session.core.recoverIncomplete();if(text)session.binding.recover(text);}
        session.core.retry();if(session.core.error)throw Error(session.core.error);
      }
      const target=this.choose();if(target.profile.inputProtocol===3)await this.liveStream(target).ready();if(generation!==this.generation)return;
      this.segments=this.events();await microphone.open((pcm,at)=>{try{this.segments?.push(pcm,at);}catch(e){this.pause(e instanceof Error?e.message:'语音输入失败');}},message=>this.pause(message));
      void this.pump();this.publish();
    }catch(e){if(generation===this.generation)this.pause(e instanceof Error?e.message:'麦克风开启失败');}
    finally{if(generation===this.generation){this.opening=false;this.publish();}}
  };
  private async pump(){
    if(this.working===this.generation||this.error)return;this.working=this.generation;
    const generation=this.generation;
    try{
      while(this.jobs.length&&!this.error){
        const job=this.jobs[0],{session,profile}=job.target,b=session.binding;
        const draft=session.core.drafts.find(d=>d.utteranceId===job.id);
        const control=new AbortController();this.transport=control;
        try{
          const transcript=await settingsRpc('transcribe',VoiceTranscribeRequestSchema,{
            deviceId:b.deviceId,agentId:b.agentId,configurationRevision:profile.revision,sessionId:this.captureId,
            utteranceId:job.id,wav:job.bytes,previousText:draft?.text||'',
            context:profile.inputProtocol===VoiceInputProtocol.AUDIO_CHAT?session.core.recognitionContext:[],
          },VoiceTranscriptSchema,'voice',control.signal);
          if(generation!==this.generation)return;
          const revision=(draft?.revision||0)+1,literal=[draft?.literal,transcript.literal].filter(Boolean).join(' ');
          session.core.transcript({utteranceId:job.id,revision,literal,final:job.final,correction:{baseRevision:draft?.revision||0,text:transcript.text},correctsRequestId:transcript.correctsRequestId||undefined});
          this.jobs.shift();
          if(session.core.error)throw Error(session.core.error);
          this.publish();
        }catch(e){if(generation===this.generation&&!control.signal.aborted)this.pause(e instanceof Error?e.message:'语音识别失败');return;}
        finally{if(this.transport===control)this.transport=null;}
      }
    }finally{if(this.working===generation)this.working=0;}
  }
}
