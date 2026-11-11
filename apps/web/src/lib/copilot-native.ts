import {create,fromBinary,toBinary,type MessageInitShape} from '@bufbuild/protobuf';
import {NativeVoiceRequestSchema,NativeVoiceResponseSchema} from '@ai-phone/protocol/ai_phone/v1/voice_pb';
import {SpeechSegments} from './copilot-audio';

type NativeCore={invoke:(command:string,body:Uint8Array)=>Promise<ArrayBuffer|number[]>};
function nativeCore(){
  return (globalThis as typeof globalThis&{__TAURI__?:{core:NativeCore}}).__TAURI__?.core;
}
export const hasNativeSpeech=()=>Boolean(nativeCore());
type VoiceEvent=
  |{type:'speechstart';utteranceId:string;at:number}
  |{type:'speechend';utteranceId:string;at:number;lastSpeechAt:number}
  |{type:'transcript';utteranceId:string;text:string;revision:number;final:boolean;at:number;inferenceMs:number}
  |{type:'error';message:string};
type Turn={id:string;frames:Int16Array[];samples:number;revision:number;lastDecodeAt?:number;final:boolean;lastSpeechAt:number};

export class NativeRecognition{
  private readonly sessionId=crypto.randomUUID();
  private enabled=false;
  private segments:SpeechSegments<Turn>|null=null;
  private turns:Turn[]=[];
  private running=false;
  private dispatch:(event:VoiceEvent)=>void=()=>{};
  private async request(action:MessageInitShape<typeof NativeVoiceRequestSchema>['action']){
    const core=nativeCore();
    if(!core)throw Error('当前客户端不支持本机识别');
    const body=toBinary(NativeVoiceRequestSchema,create(NativeVoiceRequestSchema,{sessionId:this.sessionId,action}));
    return fromBinary(NativeVoiceResponseSchema,new Uint8Array(await core.invoke('native_voice',body)));
  }
  async open(language:string,dispatch:(event:VoiceEvent)=>void){
    this.dispatch=dispatch;
    this.enabled=true;
    await this.request({case:'start',value:{language}});
    if(!this.enabled)return;
    this.segments=new SpeechSegments<Turn>({
      choose:()=>({id:'',frames:[],samples:0,revision:0,final:false,lastSpeechAt:0}),
      start:(id,turn,at)=>{
        turn.id=id;turn.lastSpeechAt=at;this.turns.push(turn);
        dispatch({type:'speechstart',utteranceId:id,at});
      },
      speech:(_id,turn,at)=>{turn.lastSpeechAt=at;},
      chunk:()=>{},
      stream:(_id,turn,frames,final)=>{
        turn.frames.push(...frames);
        turn.samples+=frames.reduce((total,frame)=>total+frame.length,0);
        turn.final=final;
        void this.pump();
      },
      end:(id,turn,at)=>dispatch({type:'speechend',utteranceId:id,at,lastSpeechAt:turn.lastSpeechAt}),
    },()=>crypto.randomUUID(),()=>false,()=>true,650);
  }
  push(pcm:Int16Array,at:number){this.segments?.push(pcm,at);}
  close(){
    this.enabled=false;
    this.segments=null;
    this.turns=[];
    return this.request({case:'stop',value:true});
  }
  private async pump(){
    if(this.running||!this.enabled)return;
    const turn=this.turns.find(turn=>turn.final||(turn.lastDecodeAt===undefined
      ?turn.samples>=8000:turn.lastSpeechAt-turn.lastDecodeAt>=500));
    if(!turn)return;
    this.running=true;
    const pcm=new Uint8Array(turn.samples*2),view=new DataView(pcm.buffer);
    let offset=0;
    for(const frame of turn.frames)for(const sample of frame){view.setInt16(offset,sample,true);offset+=2;}
    turn.lastDecodeAt=turn.lastSpeechAt;
    const final=turn.final;
    try{
      const result=await this.request({case:'audio',value:{pcm16:pcm}});
      if(!this.enabled)return;
      this.dispatch({type:'transcript',utteranceId:turn.id,text:result.text,revision:++turn.revision,final,at:performance.now(),inferenceMs:result.inferenceMs});
      if(final)this.turns.splice(this.turns.indexOf(turn),1);
    }catch(error){
      if(this.enabled){this.enabled=false;this.dispatch({type:'error',message:error instanceof Error?error.message:String(error)});}
    }finally{
      this.running=false;
      void this.pump();
    }
  }
}
