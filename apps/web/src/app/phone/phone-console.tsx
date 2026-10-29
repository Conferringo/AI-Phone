"use client";
import {useI18n} from '@/components/locale-provider';
import {DeviceDefaultsProvider,creationDefaults,type DeviceDefaults} from '@/components/device-defaults';
import { useCallback, useEffect, useRef, useState,type CSSProperties,type Dispatch,type SetStateAction } from "react";
import {
  create,
  fromBinary,
  toBinary,
  type DescMessage,
  type MessageShape,
  type MessageInitShape,
} from "@bufbuild/protobuf";
import {
  DeviceRequestSchema,
  DeviceObservationSchema,
  DeviceActionRequestSchema,
  DeviceActionResponseSchema,
  TakeControlRequestSchema,
  ControlStateSchema,
} from "@ai-phone/protocol/ai_phone/v1/phone_runtime_pb";
import {
  ChatHistoryRequestSchema,
  ChatMessageSchema,
  ChatStateSchema,
  ChatRunRequestSchema,
  ChatAttachmentSchema,
} from "@ai-phone/protocol/ai_phone/v1/assistant_pb";
import { ApiErrorSchema } from "@ai-phone/protocol/ai_phone/v1/system_pb";
import {
  ArrowUp,
  ArrowLeft,
  ArrowUpRight,
  Plus,
  Check,
  Paperclip,
  Square,
  Play,
  X,
  LoaderCircle,
  Smartphone,
  Monitor,
  MessageSquare,
  Wrench,
  BookOpen,
  FolderOpen,
  RefreshCw,
  Settings,
  Compass,
  Cable,
} from "lucide-react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import "./phone.css";
import "./device-workbench.css";
import {useWorkbenchMotion} from "@/components/workbench-motion";
import {DeviceCableLine} from "@/components/device-cable-line";
import {focusGroups} from "@/lib/device-focus";
import { SkillsWindow } from "@/components/skills-window";
import { KnowledgeWindow } from "@/components/knowledge-window";
import { WorkspaceWindow } from "@/components/workspace-window";
import { ExecutionTrace } from "@/components/execution-trace";
import {ProactiveProvider,ExplorationToggle,ProactiveWindow} from "@/components/proactive-controls";
import { RegistryWindow,type Registry } from "@/components/registry-window";
import { FloatingWindow, WindowTrigger } from "@/components/floating-window";
import {DesktopRequestSchema,DesktopObservationSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {DeviceCable} from '@/components/device-cable';
import {useDesktopExploration,DesktopExploreToggle,DesktopExploreWindow} from '@/components/desktop-exploration';
import {DesktopSurface} from '@/components/desktop-surface';
import { PhoneStreamImage } from "@/components/phone-stream-image";
import { PhoneToolbar,type RotationMode } from "@/components/phone-toolbar";
import { clientId } from "@/lib/client-id";
import {useChatOutbox} from "@/components/use-chat-outbox";
import {acceptSnapshot,type ChatSnapshot} from "@/lib/chat-snapshot";

type Screen = MessageShape<typeof DeviceObservationSchema>;
type Chat = MessageShape<typeof ChatStateSchema>;
type Attachment = MessageShape<typeof ChatAttachmentSchema>;
type PendingFile = {
  key: string;
  name: string;
  preview: string;
  file?: Attachment;
  error?: string;
};
async function rpc<I extends DescMessage, O extends DescMessage>(
  group: string,
  operation: string,
  schema: I,
  value: MessageShape<I>,
  output: O,
  signal?: AbortSignal,
): Promise<MessageShape<O>> {
  const response = await fetch(`/api/${group}/${operation}`, {
    method: "POST",
    headers: { "content-type": "application/x-protobuf" },
    body: toBinary(schema, value),
    signal,
    cache: "no-store",
  });
  const bytes = new Uint8Array(await response.arrayBuffer());
  if(response.status===401)location.assign("/login");
  if (!response.ok) throw new Error(fromBinary(ApiErrorSchema, bytes).message);
  return fromBinary(output, bytes);
}
export default function PhoneConsole({defaults=creationDefaults}:{defaults?:DeviceDefaults}) {
  return <DeviceDefaultsProvider value={defaults}><ProactiveProvider><DeviceWorkbench/></ProactiveProvider></DeviceDefaultsProvider>;
}
type SessionState={agentId:string;agentName:string;status:string;runId:string;reply:string;nextWakeAtMs:number};
function DeviceWorkbench(){
 const {t}=useI18n();

 const [registry,setRegistry]=useState<Registry|null>(null),[focus,setFocus]=useState<string[]>(['phone-local-01']),[current,setCurrent]=useState('phone-local-01');
 const [feature,setFeature]=useState(''),[states,setStates]=useState<Record<string,SessionState>>({}),[horizontal,setHorizontal]=useState<Record<string,boolean>>({}),[linked,setLinked]=useState<string[]>([]);
 const scene=useRef<HTMLElement>(null),refreshers=useRef<Record<string,()=>void>>({}),orientations=useRef<Record<string,boolean>>({});
 const devices=registry?.devices??[],groups=focusGroups(devices,focus,current,states),solo=focus.length===1,wide=!solo||!!horizontal[focus[0]]||devices.find(d=>d.id===focus[0])?.kind==='desktop';
 const receive=useCallback((id:string,value:SessionState)=>setStates(old=>JSON.stringify(old[id])===JSON.stringify(value)?old:{...old,[id]:value}),[]);
 const capture=useWorkbenchMotion(scene,`${focus.join('|')}:${wide}:${linked.join('|')}:${Object.entries(horizontal).map(([id,value])=>id+value).join('|')}`);
 const orientation=useCallback((id:string,value:boolean)=>{if(orientations.current[id]===value)return;orientations.current[id]=value;capture();setHorizontal(old=>({...old,[id]:value}));},[capture]);
 const register=useCallback((id:string,refresh:(()=>void)|null)=>{if(refresh)refreshers.current[id]=refresh;else delete refreshers.current[id];},[]);
 function move(update:()=>void){
  capture();
  setFeature('');update();
 }
 function enter(id:string){if(focus.length===1&&focus[0]===id){setCurrent(id);return;}move(()=>{setFocus(linked.length>1?linked:[id]);setCurrent(id);});}
 const previousLinks=useRef<string[]>([]);
 const receiveLinks=useCallback((ids:string[])=>{const previous=previousLinks.current;if(previous.join()===ids.join())return;capture();previousLinks.current=ids;setLinked(ids);if(previous.length&&ids.length===0){setFocus([current]);setFeature('');}else if(ids.length>1){setFocus(ids);}},[current,capture]);
 const close=(name:string)=>setFeature(old=>old===name?'':old);
 return <main ref={scene} className="device-workbench" data-runtime={focus.includes('phone-local-01')}>
  <header className="workbench-navigation">
   <nav className="workbench-shared-tools" aria-label={t("工作工具")}>
    <WindowTrigger label={t("Skill管理")} open={feature==='skills'} onOpen={()=>setFeature('skills')}><Wrench size={18}/></WindowTrigger>
    <WindowTrigger label={t("知识库")} open={feature==='knowledge'} onOpen={()=>setFeature('knowledge')}><BookOpen size={18}/></WindowTrigger>
    <WindowTrigger label={t("工作文件")} open={feature==='files'} onOpen={()=>setFeature('files')}><FolderOpen size={18}/></WindowTrigger>
    <WindowTrigger label={t("配置中心")} open={feature==='settings'} onOpen={()=>setFeature('settings')}><Settings size={18}/></WindowTrigger>
   </nav>
   <Button variant="ghost" size="icon" aria-label={t("刷新对话显示")} title={t("刷新对话显示")} disabled={!states[current]} onClick={()=>refreshers.current[current]?.()}><RefreshCw size={18}/></Button>
  </header>
  <div className="device-focus-grid" data-stacked={solo} data-wide={wide} data-count={focus.length} data-mixed={focus.length>1&&focus.some(id=>devices.find(d=>d.id===id)?.kind==='phone'&&!horizontal[id])} style={{'--device-columns':solo?'minmax(360px,1.65fr)':focus.map(id=>devices.find(d=>d.id===id)?.kind==='desktop'?'minmax(220px,1.3fr)':'minmax(180px,.8fr)').join(' '),'--chat-column':focus.length+1,'--chat-rows':groups.map(()=> 'minmax(0,1fr)').join(' ')} as CSSProperties}>
   {devices.map((d,position)=>{const index=focus.indexOf(d.id),group=groups.findIndex(g=>g.deviceIds.includes(d.id)),showChat=group>=0&&groups[group].chatDeviceId===d.id;return <PhoneWorkspace key={d.id} deviceId={d.id} registry={registry!} focused={index>=0} onSelect={()=>enter(d.id)} linked={linked.includes(d.id)} stageStyle={{'--wide-screen-column':`${Math.max(0,index)*6/Math.max(1,focus.length)+1} / span ${6/Math.max(1,focus.length)}`,'--wide-chat-column':`${Math.max(0,group)*6/Math.max(1,groups.length)+1} / span ${6/Math.max(1,groups.length)}`,'--conversation-index':Math.max(0,group),'--stage-index':position,'--stage-side':index>=0?0:(devices.length===2?(d.kind==='desktop'?1:-1):((position-devices.findIndex(v=>v.id===current)+devices.length)%devices.length===1?1:-1)),'--stage-depth':index>=0?0:-180} as CSSProperties} current={current===d.id} showChat={showChat} screenColumn={Math.max(1,index+1)} chatColumn={focus.length+1} chatRow={group+1} onActivate={id=>setCurrent(id)} onState={receive} onOrientation={orientation} registerRefresh={register} feature={feature} setFeature={setFeature}/>;})}
  </div>
  <DeviceCableLine root={scene} devices={devices} linked={linked}/>
  <RegistryWindow open={feature==='settings'} onClose={()=>close('settings')} onNeedsSetup={()=>setFeature('settings')} onChange={setRegistry} activeDeviceId={current}/>
  <DeviceCable devices={devices} current={current} open={feature===`cable:${current}`} onClose={()=>close(`cable:${current}`)} onLinks={receiveLinks}/>
  <SkillsWindow open={feature==='skills'} onClose={()=>close('skills')}/><KnowledgeWindow open={feature==='knowledge'} onClose={()=>close('knowledge')}/><WorkspaceWindow open={feature==='files'} onClose={()=>close('files')}/>
 </main>;
}
type SessionProps={linked:boolean;stageStyle:CSSProperties;onSelect:()=>void;deviceId:string;registry:Registry;focused:boolean;current:boolean;showChat:boolean;screenColumn:number;chatColumn:number;chatRow:number;onActivate:(id:string)=>void;onState:(id:string,value:SessionState)=>void;onOrientation:(id:string,value:boolean)=>void;registerRefresh:(id:string,refresh:(()=>void)|null)=>void;feature:string;setFeature:Dispatch<SetStateAction<string>>};
function PhoneWorkspace({linked,stageStyle,onSelect,deviceId,registry,focused,current,showChat,screenColumn,chatColumn,chatRow,onActivate,onState,onOrientation,registerRefresh,feature,setFeature}:SessionProps) {
 const {t}=useI18n();

  const selectedDevice=registry.devices.find(d=>d.id===deviceId)!;
  const phoneAvailable=selectedDevice.kind==='phone'&&selectedDevice.runtimeState==='managed',desktopAvailable=selectedDevice.kind==='desktop'&&selectedDevice.runtimeState==='managed',runtimeAvailable=phoneAvailable||desktopAvailable;
  const scope={phoneId:deviceId,ownerId:'local-user'};
  const desktopExploration=useDesktopExploration(deviceId,runtimeAvailable&&deviceId!=='phone-local-01');

  const [image, setImage] = useState("");
  const [phoneError, setPhoneError] = useState("");
  const screen = useRef<Screen | null>(null);
  const imageUrl = useRef("");
  const observing = useRef(false);
  const streamHealthy=useRef(false);
  const [visible,setVisible]=useState(true);
  const acting = useRef(false);
  const pointer = useRef<{ x: number; y: number; at: number; movedAt?: number; kind: string } | null>(null);
  const wheelAt = useRef(0);
  const [chatSnapshot,setChatSnapshot]=useState<ChatSnapshot>({chat:null,order:0});
  const chat=chatSnapshot.chat;
  const [displayCut,setDisplayCut]=useState<{ids:string[];runId:string;at:number;text:string}|null>(null);
  useEffect(()=>{try{const value=JSON.parse((localStorage.getItem(`ai-phone:chat-display-cut:${deviceId}`)??(deviceId==='phone-local-01'?localStorage.getItem('ai-phone:chat-display-cut'):null))||'null');if(value&&Array.isArray(value.ids)&&value.ids.every((id:unknown)=>typeof id==='string')&&typeof value.at==='number'&&typeof value.runId==='string')setDisplayCut(value);}catch{}},[deviceId]);
  function refreshChatDisplay(){
    if(!chat)return;
    const value={ids:chat.messages.map(m=>m.id),runId:['running','queued','paused'].includes(chat.runStatus)?chat.runId:'',at:Date.now(),text:chat.messages.find(m=>m.role==='assistant'&&m.runId===chat.runId)?.text||''};
    setDisplayCut(value);try{localStorage.setItem(`ai-phone:chat-display-cut:${deviceId}`,JSON.stringify(value));}catch{}
    setActiveFeature('');setError('');setHistoryError('');follow.current=true;
  }
  const conversation = useRef("");
  const fetchingChat = useRef(false);
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  const [historyError, setHistoryError] = useState("");
  const historyGeneration=useRef(0);
  const beginHistory=useCallback(()=>++historyGeneration.current,[]);
  const acceptChat=useCallback((next:Chat,order=++historyGeneration.current)=>{
    setChatSnapshot(previous=>acceptSnapshot(previous,next,order));
  },[]);
  const outbox=useChatOutbox(chat?.conversationId||'',deviceId,acceptChat,beginHistory);
  const [files, setFiles] = useState<PendingFile[]>([]);
  const fileInput = useRef<HTMLInputElement>(null);
  const [preview, setPreview] = useState<Attachment | null>(null);
  const [captureFile,setCaptureFile]=useState<File|null>(null),[captureUrl,setCaptureUrl]=useState(''),[captureError,setCaptureError]=useState('');
  useEffect(()=>{if(!captureFile){setCaptureUrl('');return;}const url=URL.createObjectURL(captureFile);setCaptureUrl(url);return()=>URL.revokeObjectURL(url);},[captureFile]);
  const activeFeature=feature.endsWith(':'+deviceId)?feature.slice(0,-deviceId.length-1):'';
  function openFeature(name:string){setFeature(name+':'+deviceId);}
  function closeFeature(name:string){setFeature(value=>value===name+':'+deviceId?'':value);}
  function setActiveFeature(name:string){setFeature(name?name+':'+deviceId:'');}
  const phoneInput=useRef<HTMLTextAreaElement>(null),phoneComposing=useRef(false),lastComposition=useRef<{value:string;at:number}|null>(null),inputQueue=useRef<Promise<void>>(Promise.resolve()),inputBuffer=useRef(""),inputTimer=useRef<ReturnType<typeof setTimeout>|null>(null);
  const [landscape,setLandscape]=useState(false),[size,setSize]=useState({width:720,height:1440});
  const orientation=useRef(false),container=useRef<HTMLDivElement>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const follow = useRef(true);

  const active =
    !!chat && ["queued", "running", "paused"].includes(chat.runStatus);
  const paused = chat?.runStatus === "paused";
  useEffect(()=>{onState(deviceId,{agentId:(active&&chat?.agentId)||selectedDevice.defaultAgentId,agentName:(active&&chat?.agentName)||registry.agents.find(a=>a.id===selectedDevice.defaultAgentId)?.name||t("未分配 Agent"),status:chat?.runStatus||'',runId:chat?.runId||'',reply:chat?.messages.filter(m=>m.role==='assistant'&&m.runId===chat.runId).at(-1)?.text||'',nextWakeAtMs:Number(chat?.nextWakeAtMs||0)});},[chat,active,deviceId,selectedDevice.defaultAgentId,registry.agents,onState]);
  useEffect(()=>{registerRefresh(deviceId,refreshChatDisplay);return()=>registerRefresh(deviceId,null);},[chat,deviceId,registerRefresh]);
  useEffect(()=>{if(!focused){setPreview(null);pointer.current=null;}},[focused]);

  const visibleMessages=chat?.messages.filter(message=>!displayCut?.ids.includes(message.id)||(message.role==='assistant'&&message.runId===displayCut.runId))??[];
  const received=new Set(chat?.messages.map(message=>message.requestId));
  const pendingMessages=outbox.entries.filter(item=>!received.has(item.id));
  const displayedMessages=[...visibleMessages,...pendingMessages.map(item=>create(ChatMessageSchema,{id:item.id,requestId:item.id,role:'user',text:item.text,attachments:item.attachments.map(file=>({...file,sizeBytes:BigInt(file.sizeBytes)}))}))]
    .filter(message=>message.role!=='assistant'||message.text||message.hasActivity)
    .map(message=>({...message,text:displayCut?.ids.includes(message.id)&&message.text===displayCut.text?'':message.text}));
  const observe = useCallback(async (signal?: AbortSignal) => {
    if (!focused || !phoneAvailable || observing.current) return;
    observing.current = true;
    try {
      const next = await rpc(
        "phone",
        "observe",
        DeviceRequestSchema,
        create(DeviceRequestSchema, {...scope,metadataOnly:streamHealthy.current}),
        DeviceObservationSchema,
        signal,
      );
      if (signal?.aborted) return;
      const horizontal=next.width>next.height;
      if(horizontal!==orientation.current){onOrientation(deviceId,horizontal);orientation.current=horizontal;setLandscape(horizontal);}
      setSize(previous=>previous.width===next.width&&previous.height===next.height?previous:{width:next.width,height:next.height});
      if(!next.png.length){screen.current=next;setPhoneError("");return;}
      const url = URL.createObjectURL(
        new Blob([new Uint8Array(next.png)], { type: "image/png" }),
      );
      const previous = imageUrl.current;
      imageUrl.current = url;
      screen.current = next;
      setImage(url);
      setPhoneError("");
      if (previous) URL.revokeObjectURL(previous);
    } catch (e) {
      if (!signal?.aborted)
        setPhoneError(e instanceof Error ? e.message : t("手机连接中断"));
    } finally {
      observing.current = false;
    }
  }, [deviceId,phoneAvailable,focused]);
  const fetchChat = useCallback(async (signal?: AbortSignal) => {
    if (fetchingChat.current) return;
    fetchingChat.current = true;
    const requested = conversation.current, generation=beginHistory();
    try {
      const next = await rpc(
        "assistant",
        "history",
        ChatHistoryRequestSchema,
        create(ChatHistoryRequestSchema, { conversationId: requested,deviceId }),
        ChatStateSchema,
        signal,
      );
      if (signal?.aborted || requested !== conversation.current) return;
      conversation.current = next.conversationId;
      acceptChat(next,generation);
      setHistoryError("");
    } catch (e) {
      if (!signal?.aborted)
        setHistoryError(e instanceof Error ? e.message : t("读取对话失败"));
    } finally {
      fetchingChat.current = false;
    }
  }, [deviceId,acceptChat,beginHistory]);
  useEffect(() => {
    const abort=new AbortController();let phoneTimer:ReturnType<typeof setTimeout>,chatTimer:ReturnType<typeof setTimeout>;
    const phone=async()=>{if(focused&&document.visibilityState==="visible")await observe(abort.signal);if(!abort.signal.aborted)phoneTimer=setTimeout(phone,streamHealthy.current?500:250);};
    const messages=async()=>{if((focused||current)&&document.visibilityState==="visible")await fetchChat(abort.signal);if(!abort.signal.aborted)chatTimer=setTimeout(messages,700);};
    const visibility=()=>{const shown=document.visibilityState==="visible";setVisible(shown);if(!shown)streamHealthy.current=false;else {void observe(abort.signal);void fetchChat(abort.signal);}};
    void phone();void messages();document.addEventListener("visibilitychange",visibility);
    return()=>{abort.abort();clearTimeout(phoneTimer);clearTimeout(chatTimer);document.removeEventListener("visibilitychange",visibility);};
  },[observe,fetchChat,focused,current]);
  useEffect(
    () => () => {
      if (imageUrl.current) URL.revokeObjectURL(imageUrl.current);
    },
    [],
  );
  useEffect(() => {
    if (follow.current && scroll.current)
      scroll.current.scrollTop = scroll.current.scrollHeight;
  }, [chat,outbox.entries,focused,showChat]);
  const act = async (
    action: MessageInitShape<typeof DeviceActionRequestSchema>["action"],
  ) => {
    if (!focused || !phoneAvailable || acting.current || !screen.current) return false;
    acting.current = true;
    try {
      let current = screen.current;
      {
        const control = await rpc(
          "phone",
          "control",
          TakeControlRequestSchema,
          create(TakeControlRequestSchema, { ...scope, controller: "manual" }),
          ControlStateSchema,
        );
        current = {
          ...current,
          controlEpoch: control.controlEpoch,
          controller: control.controller,
        };
        screen.current = current;
      }
      const result = await rpc(
        "phone",
        "action",
        DeviceActionRequestSchema,
        create(DeviceActionRequestSchema, {
          ...scope,
          runId: "manual",
          actionId: clientId(),
          controlEpoch: current.controlEpoch,
          controller: "manual",
          action,
        }),
        DeviceActionResponseSchema,
      );
      if (!result.success)
        throw new Error(
          result.resultUnknown
            ? t("操作结果未知，请先检查画面")
            : result.output || t("操作未完成"),
        );
      await observe();
      return true;
    } catch (e) {
      setPhoneError(e instanceof Error ? e.message : t("操作失败"));
      return false;
    } finally {
      acting.current = false;
    }
  };
  async function rotatePhone(mode:RotationMode){const automatic=mode==="automatic",rotation=mode==="landscape"?1:0;if(!await act({case:"orientation",value:{automatic,rotation}}))throw Error(t("旋转未完成，请检查手机当前状态"));}
  async function capturePhone(){
    setCaptureFile(null);setCaptureError('');openFeature('phone-capture');
    try{
      const frame=desktopAvailable?await rpc('desktop','observe',DesktopRequestSchema,create(DesktopRequestSchema,{deviceId}),DesktopObservationSchema):await rpc('phone','observe',DeviceRequestSchema,create(DeviceRequestSchema,scope),DeviceObservationSchema);
      if(!frame.png.length)throw Error(t("没有收到设备画面"));
      setCaptureFile(new File([new Uint8Array(frame.png)],`${desktopAvailable?'desktop':'phone'}-${new Date().toISOString().replaceAll(':','-')}.png`,{type:'image/png'}));
    }catch(issue){setCaptureError(issue instanceof Error?issue.message:t("截图失败"));}
  }
  function queuePhone(action:MessageInitShape<typeof DeviceActionRequestSchema>["action"]){inputQueue.current=inputQueue.current.catch(()=>{}).then(async()=>{const deadline=Date.now()+10000;while(acting.current&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,20));if(acting.current)throw new Error(t("手机输入等待超时"));if(!await act(action))throw new Error(t("手机输入失败，请检查输入框"));}).catch(e=>setPhoneError(e instanceof Error?e.message:t("手机输入失败")));}
  function flushPhone(){if(inputTimer.current)clearTimeout(inputTimer.current);const value=inputBuffer.current;inputBuffer.current="";if(value)for(let start=0,letters=Array.from(value);start<letters.length;start+=4000)queuePhone({case:"input",value:{text:letters.slice(start,start+4000).join("")}});}
  function typePhone(value:string){inputBuffer.current+=value;if(inputTimer.current)clearTimeout(inputTimer.current);inputTimer.current=setTimeout(flushPhone,70);}
  useEffect(()=>()=>{if(inputTimer.current)clearTimeout(inputTimer.current);},[]);
  function point(e: React.PointerEvent<HTMLImageElement>) {
    const r = e.currentTarget.getBoundingClientRect(),
      w = screen.current?.width ?? 720,
      h = screen.current?.height ?? 1440;
    return {
      x: Math.max(
        0,
        Math.min(w - 1, Math.round(((e.clientX - r.left) / r.width) * w)),
      ),
      y: Math.max(
        0,
        Math.min(h - 1, Math.round(((e.clientY - r.top) / r.height) * h)),
      ),
    };
  }
  const upload = async (selected: File[]) => {
    if (files.length + selected.length > 8) {
      setError(t("每条消息最多8个附件"));
      return;
    }
    for (const file of selected) {
      const key = clientId(),
        preview = file.type.startsWith("image/")
          ? URL.createObjectURL(file)
          : "";
      setFiles((old) => [...old, { key, name: file.name, preview }]);
      try {
        const body = new FormData();
        body.append("file", file);
        const response = await fetch("/api/assistant/upload", {
          method: "POST",
          body,
        });
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (!response.ok)
          throw new Error(fromBinary(ApiErrorSchema, bytes).message);
        const uploaded = fromBinary(ChatAttachmentSchema, bytes);
        setFiles((old) =>
          old.map((f) => (f.key === key ? { ...f, file: uploaded } : f)),
        );
      } catch (e) {
        setFiles((old) =>
          old.map((f) =>
            f.key === key
              ? { ...f, error: e instanceof Error ? e.message : t("上传失败") }
              : f,
          ),
        );
      }
    }
  };
  const send = () => {
    if(!runtimeAvailable||!chat||(!text.trim()&&!files.length)||files.some(file=>!file.file))return;
    try{
      outbox.submit({id:clientId(),conversationId:chat.conversationId,deviceId,text,attachments:files.map(item=>({...item.file!,sizeBytes:item.file!.sizeBytes.toString()}))});
      // Only the submitted snapshot is cleared, synchronously; acknowledgements never touch drafts.
      setText('');setFiles([]);setError('');follow.current=true;
      files.forEach(file=>{if(file.preview)URL.revokeObjectURL(file.preview);});
    }catch(value){setError(value instanceof Error?value.message:t('发送失败'));}
  };
  const runCommand = async (operation: "cancel" | "resume") => {
    if (!chat?.runId) return;
    try {
      const next = await rpc(
        "assistant",
        operation,
        ChatRunRequestSchema,
        create(ChatRunRequestSchema, { runId: chat.runId }),
        ChatStateSchema,
      );
      acceptChat(next);
      setError("");
    } catch (e) {
      setError(e instanceof Error ? e.message : t("操作失败"));
    }
  };
  return (
    <div ref={container} className="device-session" data-landscape={landscape} onPointerDownCapture={()=>{if(focused)onActivate(deviceId);}} onFocusCapture={()=>{if(focused)onActivate(deviceId);}}>
      <div className="workspace-panes">
        <section className="device-pane" onClick={()=>{if(!focused)onSelect();}} data-focused={focused} data-kind={selectedDevice.kind} aria-label={selectedDevice.name} style={{...stageStyle,'--device-aspect':desktopAvailable?1.6:size.width/size.height,gridColumn:screenColumn,gridRow:'1 / -1'} as CSSProperties}>
          <div className="device-visual"><header className="session-device-header" data-scene-device={`header:${deviceId}`}><span>{selectedDevice.name}</span>{runtimeAvailable&&<div className="session-device-tools" style={focused?undefined:{display:"none"}}><PhoneToolbar kind={desktopAvailable?"desktop":"phone"} deviceId={deviceId} rotate={rotatePhone} capture={capturePhone} landscape={landscape} onInteract={()=>setActiveFeature('')}/><WindowTrigger label={t("自主探索设置")} open={activeFeature==='proactive'} onOpen={()=>openFeature('proactive')}><Compass size={17}/></WindowTrigger><WindowTrigger label={t("数据线")} open={activeFeature==='cable'} onOpen={()=>openFeature('cable')}><Cable size={17} className={linked?'cable-connected':undefined}/></WindowTrigger></div>}</header>
        {!focused&&<button className="device-front" aria-label={`转到${selectedDevice.name}`} onClick={onSelect}/> }
        {desktopAvailable?<DesktopSurface deviceId={deviceId} active={focused}/>:phoneAvailable?<>
          <div className="device-screen" data-scene-device={deviceId} style={{aspectRatio:`${size.width}/${size.height}`,'--device-aspect':size.width/size.height} as CSSProperties}>
            {image || visible ? (
              <PhoneStreamImage
                endpoint={`/api/phone/media/screen?device=${encodeURIComponent(deviceId)}`}
                fallback={image}
                enabled={visible}
                onConnectionState={online=>{streamHealthy.current=online;}}
                alt={t("Android实时屏幕")}
                draggable={false}
                onPointerDown={(e) => {
                  if (!focused || e.button !== 0) return;
                  if(e.pointerType==="mouse"){e.preventDefault();phoneInput.current?.focus({preventScroll:true});}
                  e.currentTarget.setPointerCapture(e.pointerId);
                  pointer.current = { ...point(e), at: performance.now(), kind: e.pointerType };
                }}
                onWheel={(e) => {
                  const now=performance.now();
                  if(Math.abs(e.deltaY)<4 || now-wheelAt.current<400 || acting.current)return;
                  wheelAt.current=now;
                  const height=screen.current?.height ?? 1440, width=screen.current?.width ?? 720;
                  const startY=Math.round(height*.6),distance=Math.round(Math.min(height*.45,Math.max(120,Math.abs(e.deltaY)*(e.deltaMode===1?30:3))));
                  void act({case:"swipe",value:{x1:Math.round(width*.5),x2:Math.round(width*.5),y1:startY,y2:Math.max(100,Math.min(height-100,startY-Math.sign(e.deltaY)*distance)),durationMs:220}});
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  pointer.current = null;
                  const r = e.currentTarget.getBoundingClientRect();
                  void act({case: "longPress", value: {
                    x: Math.max(0, Math.min((screen.current?.width ?? 720)-1, Math.round((e.clientX-r.left)/r.width*(screen.current?.width ?? 720)))),
                    y: Math.max(0, Math.min((screen.current?.height ?? 1440)-1, Math.round((e.clientY-r.top)/r.height*(screen.current?.height ?? 1440)))),
                    durationMs: 650,
                  }});
                }}
                onPointerMove={(e) => {
                  const start = pointer.current;
                  if (start && !start.movedAt) {
                    const next = point(e);
                    if (Math.hypot(next.x-start.x,next.y-start.y)>=12) start.movedAt=performance.now();
                  }
                }}
                onPointerCancel={() => {
                  pointer.current = null;
                }}
                onPointerUp={(e) => {
                  if (!focused || e.button !== 0) return;
                  const start = pointer.current;
                  pointer.current = null;
                  if (!start) return;
                  const end = point(e);
                  if (Math.hypot(end.x - start.x, end.y - start.y) < 12)
                    void act(performance.now()-start.at>=500 ? {case:"longPress",value:{...end,durationMs:650}} : { case: "tap", value: end });
                  else
                    void act({
                      case: start.kind === "mouse" || (start.movedAt ?? performance.now())-start.at >= 400 ? "drag" : "swipe",
                      value: {
                        x1: start.x,
                        y1: start.y,
                        x2: end.x,
                        y2: end.y,
                        durationMs: Math.min(650, Math.max(120, Math.round(performance.now() - (start.movedAt ?? start.at)))) ,
                      },
                    });
                }}
              />
            ) : (
              <LoaderCircle
                className="spinning"
                size={22}
                aria-label={t("连接手机")}
              />
            )}
          </div>
          <textarea ref={phoneInput} className="phone-keyboard-proxy" aria-label={t("手机键盘输入")} autoComplete="off" autoCapitalize="off" spellCheck={false} tabIndex={-1}
            onCompositionStart={()=>{lastComposition.current=null;phoneComposing.current=true;}}
            onCompositionEnd={e=>{phoneComposing.current=false;const value=e.currentTarget.value;lastComposition.current={value,at:Date.now()};e.currentTarget.value="";if(value)typePhone(value);}}
            onChange={e=>{if(phoneComposing.current)return;const value=e.currentTarget.value;e.currentTarget.value="";if(value&&!(lastComposition.current?.value===value&&Date.now()-lastComposition.current.at<80))typePhone(value);}}
            onPaste={e=>{e.preventDefault();const value=e.clipboardData.getData("text/plain");if(value)typePhone(value);}}
            onKeyDown={e=>{if(e.nativeEvent.isComposing||phoneComposing.current||e.keyCode===229)return;if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==="a"){e.preventDefault();flushPhone();queuePhone({case:"key",value:{key:"SELECT_ALL"}});return;}if(e.metaKey||e.ctrlKey||e.altKey)return;const keys:Record<string,string>={Backspace:"DEL",Delete:"FORWARD_DEL",Enter:"ENTER",Tab:"TAB",Escape:"BACK",ArrowLeft:"DPAD_LEFT",ArrowRight:"DPAD_RIGHT",ArrowUp:"DPAD_UP",ArrowDown:"DPAD_DOWN"};if(keys[e.key]){e.preventDefault();flushPhone();queuePhone({case:"key",value:{key:keys[e.key]}});}}}/>
          {phoneError && (
            <button className="connection-error" onClick={() => void observe()}>
              {phoneError}
            </button>
          )}
        </>:<div className="device-pending" data-scene-device={deviceId}>{selectedDevice?.kind==='desktop'?<Monitor size={32}/>:<Smartphone size={32}/>}<h2>{selectedDevice?.name||t("设备")}</h2><p>{t("尚未安装系统")}</p></div>}
        </div></section>
        <section data-scene-device={`chat:${deviceId}`} className="conversation-pane" aria-label={`${selectedDevice.name}对话`} data-chat-visible={showChat} aria-hidden={!showChat} inert={!showChat} style={{...stageStyle,display:showChat?undefined:'none',gridColumn:chatColumn,gridRow:chatRow}}>
          <div className="conversation-device-label"><span>{selectedDevice.name}</span><small>{(active&&chat?.agentName)||registry.agents.find(a=>a.id===selectedDevice.defaultAgentId)?.name||t("未分配 Agent")}</small></div>
          <div
            className="message-scroll"
            ref={scroll}
            onScroll={() => {
              const e = scroll.current;
              if (e)
                follow.current =
                  e.scrollHeight - e.scrollTop - e.clientHeight < 80;
            }}
          >
            <div className="message-list">
              {displayedMessages.map((message) => (
                <article
                  key={message.requestId||message.id}
                  className={`message message-${message.role}`}
                  aria-label={message.role === "user" ? t("你") : t("助手")}
                >
                  {message.role==="assistant"&&message.runId&&message.hasActivity&&<ExecutionTrace messageId={message.id} afterMs={message.runId===displayCut?.runId?displayCut.at:0} runId={message.runId} active={message.active&&!paused} status={message.active?chat?.runStatus:undefined} nextWakeAtMs={message.active?Number(chat?.nextWakeAtMs):0}/>}
                  {message.attachments.length > 0 && (
                    <div className="message-attachments">
                      {message.attachments.map((file) =>
                        file.mediaType.startsWith("image/") ? (
                          <button
                            key={file.id}
                            onClick={() => setPreview(file)}
                            aria-label={t("查看") + file.name}
                          >
                            <img
                              src={`/api/assistant/attachment/${file.id}`}
                              alt={file.name}
                            />
                          </button>
                        ) : (
                          <a
                            key={file.id}
                            href={`/api/assistant/attachment/${file.id}`}
                            download={file.name}
                          >
                            {file.name}
                          </a>
                        ),
                      )}
                    </div>
                  )}
                  {message.text&&<Markdown remarkPlugins={[remarkGfm]}>{message.text}</Markdown>}
                  {pendingMessages.some(item=>item.id===message.requestId)&&<span className="message-delivery" role="status">{t(pendingMessages.find(item=>item.id===message.requestId)?.status==='failed'?'未发送':'发送中')}</span>}
                </article>
              ))}
            </div>
          </div>
          <form
            className="chat-composer"
            onSubmit={(e) => {
              e.preventDefault();
              void send();
            }}
            onPaste={(e) => {
              const images = [...e.clipboardData.files].filter((file) =>
                file.type.startsWith("image/"),
              );
              if (images.length) {
                e.preventDefault();
                void upload(images);
              }
            }}
            onDragOver={(e) => {
              if (e.dataTransfer.types.includes("Files")) e.preventDefault();
            }}
            onDrop={(e) => {
              if (e.dataTransfer.files.length) {
                e.preventDefault();
                void upload([...e.dataTransfer.files]);
              }
            }}
          >
            {files.length > 0 && (
              <div className="upload-list">
                {files.map((file) => (
                  <div className="upload-item" key={file.key}>
                    {file.preview && <img src={file.preview} alt="" />}
                    <span>{file.name}</span>
                    {!file.file && !file.error && (
                      <LoaderCircle className="spinning" size={14} />
                    )}
                    <Button
                      variant="ghost"
                      size="icon"
                      type="button"
                      aria-label={t("移除") + file.name}
                      onClick={() => {
                        if (file.preview) URL.revokeObjectURL(file.preview);
                        setFiles((old) =>
                          old.filter((f) => f.key !== file.key),
                        );
                      }}
                    >
                      <X size={14} />
                    </Button>
                    {file.error && (
                      <span className="upload-error" role="alert">
                        {file.error}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            )}
            <textarea
              aria-label={t("消息")}
              placeholder={runtimeAvailable?t("发送消息"):t("设备安装后即可对话")}
              disabled={!runtimeAvailable}
              value={text}
              maxLength={12000}
              rows={3}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (
                  e.key === "Enter" &&
                  (e.metaKey || e.ctrlKey) &&
                  !e.nativeEvent.isComposing && e.nativeEvent.keyCode!==229
                ) {
                  e.preventDefault();
                  void send();
                }
              }}
            />
            <div className="composer-actions">
              <input
                type="file"
                ref={fileInput}
                hidden
                multiple
                accept="image/png,image/jpeg,image/webp,image/gif,.txt,.md,.csv,.json,.log"
                onChange={(e) => {
                  void upload([...(e.target.files ?? [])]);
                  e.target.value = "";
                }}
              />
              <Button
                variant="ghost"
                size="icon"
                type="button"
                aria-label={t("添加附件")}
                disabled={!runtimeAvailable}
                onClick={() => fileInput.current?.click()}
              >
                <Paperclip size={19} />
              </Button>
              {phoneAvailable&&deviceId==='phone-local-01'&&<ExplorationToggle/>}{runtimeAvailable&&deviceId!=='phone-local-01'&&<DesktopExploreToggle control={desktopExploration} focused={focused}/>}
              <div className="composer-submit">
                {active&&!paused&&<span className="composer-progress" role="status"><span className="composer-pulse"/>{t(chat?.runStatus==='queued'?'待继续':'正在处理')}</span>}
                {paused && (
                  <Button
                    variant="ghost"
                    size="icon"
                    type="button"
                    aria-label={t("继续任务")}
                    onClick={() => void runCommand("resume")}
                  >
                    <Play size={18} />
                  </Button>
                )}
                {active && (
                  <Button
                    variant="ghost"
                    size="icon"
                    type="button"
                    aria-label={t("停止任务")}
                    onClick={() => void runCommand("cancel")}
                  >
                    <Square size={14} fill="currentColor" />
                  </Button>
                )}
                {(
                  <Button
                    className="send-button"
                    size="icon"
                    type="submit"
                    aria-label={active?t("补充任务指令"):t("发送")}
                    disabled={
                      !outbox.ready ||
                      !chat ||
                      (!text.trim() && !files.length) ||
                      files.some((f) => !f.file)
                    }
                  >
                    <ArrowUp size={19} />
                  </Button>
                )}
              </div>
            </div>
          </form>
          {(outbox.error||outbox.entries[0]?.status==='failed')&&<div className="chat-error" role="alert"><span>{outbox.error||outbox.entries[0]?.error}</span><Button variant="ghost" type="button" onClick={outbox.retry}>{t('重试')}</Button></div>}
          {(error || historyError) && (
            <div className="chat-error" role="alert">
              {error || historyError}
              <Button
                variant="ghost"
                size="icon"
                aria-label={t("关闭错误")}
                onClick={() => { setError(""); setHistoryError(""); }}
              >
                <X size={14} />
              </Button>
            </div>
          )}
        </section>
      </div>
      <FloatingWindow id={`phone-capture-${deviceId}`} title={t("截图")} open={activeFeature==='phone-capture'} onClose={()=>closeFeature('phone-capture')} width={desktopAvailable?700:360} height={580} fitContent>
        <div className="phone-capture-preview">
          {captureUrl?<img src={captureUrl} alt={t("设备截图预览")}/>:captureError?<p role="alert" className="workbench-error">{captureError}</p>:<LoaderCircle className="spinning" size={20} aria-label={t("正在截图")}/>}
          {captureFile&&captureUrl&&<div className="phone-capture-actions"><a href={captureUrl} download={captureFile.name}>{t("保存图片")}</a><Button variant="secondary" onClick={()=>{void upload([captureFile]);closeFeature('phone-capture');}}>{t("附到对话")}</Button></div>}
        </div>
      </FloatingWindow>
      {phoneAvailable&&deviceId==='phone-local-01'&&<ProactiveWindow open={activeFeature==="proactive"} onClose={()=>closeFeature("proactive")}/>}
      {runtimeAvailable&&deviceId!=='phone-local-01'&&<DesktopExploreWindow control={desktopExploration} deviceId={deviceId} open={activeFeature==="proactive"} onClose={()=>closeFeature("proactive")}/>}
      <Dialog
        open={!!preview}
        onOpenChange={(open) => {
          if (!open) setPreview(null);
        }}
      >
        <DialogContent className="attachment-dialog">
          <DialogTitle>{preview?.name}</DialogTitle>
          {preview && (
            <img
              src={`/api/assistant/attachment/${preview.id}`}
              alt={preview.name}
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
