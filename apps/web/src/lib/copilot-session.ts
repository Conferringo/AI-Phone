import type {MessageShape} from '@bufbuild/protobuf';
import {VoiceSettingsRequestSchema,VoiceSettingsSchema,VoiceTranscribeRequestSchema,VoiceTranscriptSchema,VoiceSpeechRequestSchema,VoiceAudioSchema} from '@ai-phone/protocol/ai_phone/v1/voice_pb';
import {settingsRpc} from './settings-client';
import {CopilotInput,voiceOutboxInput,type VoiceCommit,type VoiceDraft} from './copilot-input';
import {SpeechSegments,wav} from './copilot-audio';
import {Microphone} from './copilot-capture';
import {CopilotOutput,type SpokenReply} from './copilot-output';
import type {OutboxInput} from './chat-outbox';

type Profile=MessageShape<typeof VoiceSettingsSchema>;
export type CopilotBinding={deviceId:string;conversationId:string;agentId:string;ready:boolean;submit:(input:OutboxInput)=>void;recover:(text:string)=>void;replies:SpokenReply[]};
type Session={binding:CopilotBinding;core:CopilotInput;recent:VoiceCommit[]};
type Target={session:Session;profile:Profile};
type AudioJob={id:string;target:Target;bytes:Uint8Array;final:boolean};
export type MicrophonePort=Pick<Microphone,'prepare'|'open'|'play'|'close'>;
export type CopilotState={enabled:boolean;opening:boolean;error:string;currentDeviceId:string;drafts:Record<string,VoiceDraft|undefined>};

export class CopilotSession {
  private bindings=new Map<string,CopilotBinding>();
  private profiles=new Map<string,Profile>();
  private sessions=new Map<string,Session>();
  private jobs:AudioJob[]=[];
  private working=0;
  private transport:AbortController|null=null;
  private microphone:MicrophonePort|null=null;
  private segments:SpeechSegments<Target>|null=null;
  private enabled=false;
  private opening=false;
  private error='';
  private outputError='';
  private currentDeviceId='';
  private captureId='';
  private generation=0;
  private endpointTimer:ReturnType<typeof setTimeout>|null=null;
  private listeners=new Set<()=>void>();
  private state:CopilotState={enabled:false,opening:false,error:'',currentDeviceId:'',drafts:{}};
  private speaker=new CopilotOutput<Target>(async(target,text,signal)=>{
    const b=target.session.binding,p=this.profiles.get(b.agentId)||target.profile;
    const audio=await settingsRpc('speech',VoiceSpeechRequestSchema,{deviceId:b.deviceId,agentId:b.agentId,configurationRevision:p.revision,text},VoiceAudioSchema,'voice',signal);
    await this.microphone?.play(audio.wav,signal);
  },message=>{this.outputError=message;this.publish();});
  constructor(private configure:(agentId:string)=>void,private createMicrophone:()=>MicrophonePort=()=>new Microphone()){}
  subscribe=(callback:()=>void)=>{this.listeners.add(callback);return()=>{this.listeners.delete(callback);};};
  snapshot=()=>this.state;
  private publish(){
    const drafts:CopilotState['drafts']={};
    for(const session of this.sessions.values())drafts[session.binding.deviceId]=session.core.drafts.findLast(d=>d.status!=='accepted');
    this.state={enabled:this.enabled,opening:this.opening,error:this.error||this.outputError,currentDeviceId:this.currentDeviceId,drafts};
    for(const listener of this.listeners)listener();
  }
  select(deviceId:string){if(this.currentDeviceId!==deviceId){this.currentDeviceId=deviceId;this.publish();}}
  register(binding:CopilotBinding){
    this.bindings.set(binding.deviceId,binding);
    for(const session of this.sessions.values())if(session.binding.deviceId===binding.deviceId&&session.binding.conversationId===binding.conversationId&&session.binding.agentId===binding.agentId)session.binding=binding;
    if(this.enabled){const profile=this.profiles.get(binding.agentId);if(profile?.configured){const target=this.target(binding,profile);this.speaker.feed(this.key(binding),target,binding.replies);}}
  }
  unregister(deviceId:string){this.bindings.delete(deviceId);}
  profileChanged(profile:Profile){this.profiles.set(profile.agentId,profile);}
  private key(binding:CopilotBinding){return `${binding.deviceId}:${binding.conversationId}:${binding.agentId}`;}
  private target(binding:CopilotBinding,profile:Profile):Target {
    const key=this.key(binding);let session=this.sessions.get(key);
    if(!session){
      const core=new CopilotInput({deviceId:binding.deviceId,conversationId:binding.conversationId,sessionId:this.captureId},commit=>{
        const owner=this.sessions.get(key);if(!owner)throw Error('语音会话已关闭');
        owner.binding.submit(voiceOutboxInput(commit));owner.recent=[commit];this.outputError='';this.speaker.release();
      });
      session={binding,recent:[],core};
      this.sessions.set(key,session);
    }
    return {session,profile};
  }
  private choose():Target {
    const binding=this.bindings.get(this.currentDeviceId);
    if(!binding?.ready)throw Error('当前设备对话正在连接');
    const profile=this.profiles.get(binding.agentId);
    if(!profile?.configured){this.configure(binding.agentId);throw Error('请先配置 Agent 的语音服务');}
    return this.target(binding,profile);
  }
  private events(){return new SpeechSegments<Target>({
    choose:()=>this.choose(),
    start:(id,target,at)=>{this.speaker.hold();target.session.core.speech(id,at);this.publish();},
    speech:(id,target,at)=>{target.session.core.speech(id,at);},
    chunk:(id,target,frames,final)=>{this.jobs.push({id,target,bytes:wav(frames),final});if(this.jobs.length>4)this.pause('语音识别积压，录音已暂停');else void this.pump();},
    end:(id,target,at)=>{
      const job=this.jobs.findLast(j=>j.id===id);
      if(job)job.final=true;
      else {const draft=target.session.core.drafts.find(d=>d.utteranceId===id);if(draft?.text)target.session.core.transcript({utteranceId:id,revision:draft.revision+1,literal:draft.literal,final:true,correction:{baseRevision:draft.revision,text:draft.text}});}
      target.session.core.advance(at);this.scheduleEndpoint();this.publish();
    },
  },()=>crypto.randomUUID(),target=>target.profile.inputProtocol===1);}
  private async loadProfiles(){
    const agents=[...new Set([...this.bindings.values()].map(b=>b.agentId).filter(Boolean))];
    await Promise.all(agents.map(async agentId=>{this.profiles.set(agentId,await settingsRpc('settings',VoiceSettingsRequestSchema,{agentId},VoiceSettingsSchema,'voice'));}));
  }
  async start(deviceId=this.currentDeviceId){
    if(this.opening)return;
    this.stop();this.select(deviceId);this.opening=true;this.error='';this.captureId=crypto.randomUUID();
    const generation=++this.generation,microphone=this.createMicrophone();this.microphone=microphone;this.publish();
    try{
      const gesture=microphone.prepare();await Promise.all([gesture,this.loadProfiles()]);
      if(generation!==this.generation)return;
      this.choose();this.segments=this.events();
      await microphone.open((pcm,at)=>{try{this.segments?.push(pcm,at);}catch(e){this.pause(e instanceof Error?e.message:'语音输入失败');}},message=>this.pause(message));
      if(generation!==this.generation)return;
      this.enabled=true;
      for(const binding of this.bindings.values()){const profile=this.profiles.get(binding.agentId);if(profile?.configured)this.speaker.feed(this.key(binding),this.target(binding,profile),binding.replies);}
      this.speaker.release();
    }catch(e){if(generation===this.generation){microphone.close();this.error=e instanceof Error?e.message:'麦克风开启失败';}}
    finally{if(generation===this.generation){this.opening=false;this.publish();}}
  }
  stop(){
    if(this.endpointTimer)clearTimeout(this.endpointTimer);this.endpointTimer=null;
    this.generation++;this.enabled=false;this.opening=false;this.transport?.abort();this.transport=null;this.speaker.close();this.microphone?.close();this.microphone=null;this.segments=null;this.jobs=[];
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
  private pause(message:string){const segments=this.segments;this.segments=null;this.error=message;segments?.finish(performance.now());this.microphone?.close();this.speaker.hold();this.publish();}
  retry=async()=>{
    if(this.opening)return;
    if(!this.enabled){await this.start(this.currentDeviceId);return;}
    if(!this.error){this.outputError='';this.speaker.release();this.publish();return;}
    this.error='';this.opening=true;const generation=this.generation,microphone=this.createMicrophone();this.microphone=microphone;this.publish();
    try{
      const gesture=microphone.prepare();await Promise.all([gesture,this.loadProfiles()]);if(generation!==this.generation)return;
      for(const job of this.jobs)job.target.profile=this.profiles.get(job.target.session.binding.agentId)||job.target.profile;
      for(const session of this.sessions.values()){session.core.retry();if(session.core.error)throw Error(session.core.error);}
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
          const transcript=await settingsRpc('transcribe',VoiceTranscribeRequestSchema,{deviceId:b.deviceId,agentId:b.agentId,configurationRevision:profile.revision,sessionId:this.captureId,utteranceId:job.id,wav:job.bytes,previousText:draft?.text||'',context:session.recent.map(c=>({requestId:c.requestId,literal:c.literal,text:c.text}))},VoiceTranscriptSchema,'voice',control.signal);
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
