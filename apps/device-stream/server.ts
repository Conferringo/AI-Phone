import {beginAudio} from './audio-transport';
import {physicalPoint,interpolate} from "./gestures";
import {desktopScreen,desktopAudio,stopDesktop} from './desktop-hub';
import * as grpc from "@grpc/grpc-js";
import {loadSync} from "@grpc/proto-loader";
import sharp from "sharp";
import http from "node:http";
import {homedir} from "node:os";
import {readFileSync,readdirSync,statSync} from "node:fs";
import {resolve} from "node:path";
import {timingSafeEqual} from "node:crypto";
import {fromBinary,toBinary} from "@bufbuild/protobuf";
import {CaptureWatchdog} from "./capture-watchdog";
import {ScreenHub,type RawFrame} from "./screen-hub";
import {ActionCursor} from "./action-cursor";
import {DeviceActionRequestSchema,DeviceCursorSchema} from "@ai-phone/protocol/ai_phone/v1/phone_runtime_pb";
const root=process.env.AI_PHONE_PROJECT_ROOT||resolve(import.meta.dir,"../..");
const deviceId=process.env.AI_PHONE_DEVICE_ID||"phone-local-01";
const serial=process.env.AI_PHONE_SERIAL||"emulator-5564";
const grpcPort=Number(process.env.AI_PHONE_GRPC_PORT||8558),port=Number(process.env.AI_PHONE_MEDIA_PORT||17865);
const runnerDir=process.env.AI_PHONE_RUNNER_DIR||root+"/data/phone-runner";
const token=readFileSync(runnerDir+"/token","utf8").trim();
const sdk=process.env.AI_PHONE_SDK_ROOT||"/opt/homebrew/share/android-commandlinetools";
const definition=grpc.loadPackageDefinition(loadSync(sdk+"/emulator/lib/emulator_controller.proto",{keepCase:false,longs:String,enums:String,defaults:true,oneofs:true}));
const Controller=(definition as any).android.emulation.control.EmulatorController;
let cachedPath="",client:any;

let nativeWidth=720,nativeHeight=1440,rotation=0,orientationAt=0,orientationPending:Promise<void>|null=null;
async function refreshOrientation(){
 if(Date.now()-orientationAt<500)return;if(orientationPending)return orientationPending;
 orientationPending=(async()=>{try{const child=Bun.spawn([sdk+"/platform-tools/adb","-P","15037","-s",serial,"shell","dumpsys","input"],{stdout:"pipe",stderr:"ignore",env:{...process.env,ANDROID_USER_HOME:root+"/data/android/user"},timeout:1800});const output=await new Response(child.stdout).text();if(await child.exited!==0)return;const found=output.match(/Viewport INTERNAL: displayId=0[^\n]*orientation=(\d)[^\n]*isActive=\[1\]/);if(found){const previous=rotation;rotation=Number(found[1]);if(previous!==rotation&&watchdog.raw&&hub.size)hub.offer(watchdog.raw,correction(watchdog.raw));nativeWidth=rotation%2?1440:720;nativeHeight=rotation%2?720:1440;}orientationAt=Date.now();}catch{}finally{orientationPending=null;}})();return orientationPending;
}
function correction(frame:any){const value=frame.format.rotation?.rotation;const delivered=typeof value==="number"?value:({PORTRAIT:0,LANDSCAPE:1,REVERSE_PORTRAIT:2,REVERSE_LANDSCAPE:3} as Record<string,number>)[value]||0;return -90*((rotation-delivered+4)%4);}

function connection(){
 const directory=homedir()+"/Library/Caches/TemporaryItems/avd/running";
 const files=readdirSync(directory).filter(f=>/^pid_.*\.ini$/.test(f)).map(f=>directory+"/"+f).sort((a,b)=>statSync(b).mtimeMs-statSync(a).mtimeMs);
 const path=files.find(path=>readFileSync(path,"utf8").split(/\r?\n/).includes("grpc.port="+grpcPort));
 if(!path)throw new Error("emulator_unavailable");
 const values=Object.fromEntries(readFileSync(path,"utf8").split(/\r?\n/).filter(l=>l.includes("=")).map(l=>{const at=l.indexOf("=");return[l.slice(0,at),l.slice(at+1)];}));
 if(!values["grpc.token"])throw new Error("emulator_auth_unavailable");
 if(cachedPath!==path){client?.close();client=new Controller("127.0.0.1:"+grpcPort,grpc.credentials.createInsecure(),{"grpc.max_receive_message_length":12*1024*1024});cachedPath=path;}
 const metadata=new grpc.Metadata();metadata.set("authorization","Bearer "+values["grpc.token"]);return {client,metadata};
}
async function call(method:string,input:any,deadlineMs=5000){const {client,metadata}=connection();return new Promise<any>((yes,no)=>client[method](input,metadata,{deadline:Date.now()+deadlineMs},(e:any,v:any)=>e?no(e):yes(v)));}
// Capture and transport are separate: one encoder, one latest pending raw frame,
// and one latest replacement for each backpressured viewer.
let screenCall:any=null,starting=false;
const hub=new ScreenHub(async(frame,angle)=>{
 const width=frame.format.width||frame.width||0,height=frame.format.height||frame.height||0;
 if(!width||!height){const w=nativeWidth>nativeHeight?1080:540,h=nativeWidth>nativeHeight?540:1080;return sharp({create:{width:w,height:h,channels:3,background:'#000000'}}).jpeg({quality:80}).toBuffer();}
 return sharp(frame.image,{raw:{width,height,channels:3}}).rotate(angle).jpeg({quality:80}).toBuffer();
},()=>stopScreen());
function stopScreen(preserveVerified=false){
 const old=screenCall;screenCall=null;hub.advance(preserveVerified);old?.cancel();watchdog.reset();
}
async function startScreen(){
 if(screenCall||starting||!hub.size)return;starting=true;
 try{await refreshOrientation();if(!hub.size||screenCall)return;const {client,metadata}=connection();const stream=client.streamScreenshot({format:'RGB888',width:540,height:1080,display:0},metadata);screenCall=stream;
  stream.on('data',(frame:RawFrame)=>{if(screenCall!==stream)return;watchdog.accept(frame);});
  const end=()=>{if(screenCall!==stream)return;screenCall=null;watchdog.reset();hub.close();};stream.on('error',end);stream.on('end',end);
 }catch{hub.close();}finally{starting=false;}
}
const watchdog=new CaptureWatchdog({size:()=>hub.size,generation:()=>hub.generation,verifiedAt:()=>hub.cached?.verifiedAt,capture:async()=>{await refreshOrientation();return call('getScreenshot',{format:'RGB888',width:540,height:1080,display:0},1800);},offer:frame=>hub.offer(frame,correction(frame)),heartbeat:()=>hub.heartbeat(),restart:()=>{stopScreen(true);void startScreen();},close:()=>hub.close()});
setInterval(()=>{if(hub.size){hub.sweep();void refreshOrientation();void watchdog.tick();}},500);
const actionCursor=new ActionCursor();
const delay=(ms:number)=>new Promise(r=>setTimeout(r,ms));
async function touch(x:number,y:number,down:boolean){const xy=physicalPoint(x,y,rotation);await call("sendTouch",{touches:[{...xy,identifier:0,pressure:down?1024:0,expiration:"EVENT_EXPIRATION_UNSPECIFIED"}]});}
async function action(body:Buffer){const request=fromBinary(DeviceActionRequestSchema,body);if(request.phoneId!==deviceId)throw Error("wrong_device");const a=request.action;let x=0,y=0,pressed=false;
 await refreshOrientation();
 actionCursor.begin(request,nativeWidth,nativeHeight);
 const recordedTouch=async(x:number,y:number,down:boolean)=>{await touch(x,y,down);actionCursor.confirm([{x,y,pointerId:0}]);};
 try{
 if(a.case==="gesture"){
  await refreshOrientation();const v=a.value;const originalRotation=rotation;let active:any[]=[];
  try{const start=performance.now();for(;;){const elapsed=Math.min(v.durationMs,performance.now()-start);const logical=v.paths.map(p=>({...interpolate(p.points,elapsed),pointerId:p.pointerId}));active=logical.map(q=>({...physicalPoint(q.x,q.y,originalRotation),identifier:q.pointerId,pressure:Math.round(v.pressure*1024)}));await call("sendTouch",{touches:active});actionCursor.confirm(logical);if(elapsed>=v.durationMs)break;await delay(16);}}
  finally{if(active.length)await call("sendTouch",{touches:active.map(p=>({...p,pressure:0}))});}return;
 }
 if(a.case==="motion"){
  const original=await call('getSensor',{target:'ACCELERATION'});if(original.status!=='OK'&&original.status!==0)throw Error('sensor_unavailable');
  const base=original.value.data as number[],v=a.value,start=performance.now();
  try{for(;;){const t=performance.now()-start;if(t>=v.durationMs)break;await call('setSensor',{target:'ACCELERATION',value:{data:[(base[0]||0)+v.amplitude*Math.sin(t/1000*v.frequencyHz*Math.PI*2),base[1]||0,base[2]||0]}});await delay(20);}}
  finally{await call('setSensor',{target:'ACCELERATION',value:{data:base}});}return;
 }
 if(a.case==="tap"||a.case==="longPress"){x=a.value.x;y=a.value.y;pressed=true;await recordedTouch(x,y,true);await delay(a.case==="tap"?12:a.value.durationMs);await recordedTouch(x,y,false);pressed=false;return;}
 if(a.case==="swipe"||a.case==="drag"){const value=a.value;x=value.x1;y=value.y1;pressed=true;await recordedTouch(x,y,true);if(a.case==="drag")await delay(500);const start=performance.now(),duration=Math.max(100,Math.min(1500,value.durationMs));
 for(;;){const progress=Math.min(1,(performance.now()-start)/duration);x=Math.round(value.x1+(value.x2-value.x1)*progress);y=Math.round(value.y1+(value.y2-value.y1)*progress);await recordedTouch(x,y,true);if(progress>=1)break;await delay(16);}await recordedTouch(x,y,false);pressed=false;return;}
 throw new Error("unsupported_action");
 }catch(error){actionCursor.finish('unknown');throw error;}finally{if(pressed)await recordedTouch(x,y,false).catch(()=>{});}

}
const server=http.createServer(async(req,res)=>{
 const expected=Buffer.from("Bearer "+token),supplied=Buffer.from(req.headers.authorization||"");if(supplied.length!==expected.length||!timingSafeEqual(supplied,expected)){res.writeHead(401);res.end();return;}
 if(req.method==="GET"&&req.url==="/desktop/audio"&&deviceId==="phone-local-01"){desktopAudio(res);return;}
 if(req.method==="GET"&&req.url==="/desktop/screen"&&deviceId==="phone-local-01"){desktopScreen(res);return;}
 if(req.method==="GET"&&req.url==="/cursor"){res.writeHead(200,{'content-type':'application/x-protobuf','cache-control':'no-store'});res.end(toBinary(DeviceCursorSchema,actionCursor.snapshot()));return;}
 if(req.method==="GET"&&req.url==="/status"){await refreshOrientation();res.writeHead(200,{"content-type":"application/json","cache-control":"no-store"});res.end(JSON.stringify({width:nativeWidth,height:nativeHeight,rotation,media:{...hub.stats(),streamActive:!!screenCall,streamFrameAgeMs:watchdog.streamAt?Date.now()-watchdog.streamAt:null,selfCheckBusy:watchdog.busy,captureFailures:watchdog.failures,streamRestarts:watchdog.restarts}}));return;}
 if(req.method==="GET"&&req.url==="/screen"){
  try{connection();}catch{res.writeHead(503);res.end();return;}
  res.writeHead(200,{'content-type':'multipart/x-mixed-replace; boundary=phone','cache-control':'private,no-store','x-content-type-options':'nosniff'});res.flushHeaders();hub.add(res);void startScreen();void watchdog.tick();return;
 }
 if(req.method==="GET"&&req.url==="/audio"){
  let stream:any;try{const {client,metadata}=connection();stream=client.streamAudio({samplingRate:"48000",channels:"Stereo",format:"AUD_FMT_S16",mode:"MODE_REAL_TIME"},metadata);}catch{res.writeHead(503);res.end();return;}
  beginAudio(res);
  stream.on("data",(packet:any)=>{if(res.writableLength>192000){res.destroy();stream.cancel();return;}if(packet.audio?.length)res.write(packet.audio);});stream.on("error",()=>res.end());stream.on("end",()=>res.end());res.on("close",()=>stream.cancel());return;
 }
 if(req.method==="GET"&&req.url==="/snapshot"){try{await refreshOrientation();const frame=await call("getScreenshot",{format:"PNG",display:0});const angle=correction(frame);const bytes=angle?await sharp(frame.image).rotate(angle).png().toBuffer():frame.image;res.writeHead(200,{"content-type":"image/png","cache-control":"no-store","x-capture-time":String(Math.round(Number(frame.timestampUs||0)/1000)),"x-verified-time":String(Date.now())});res.end(bytes);}catch{res.writeHead(503);res.end();}return;}
 if(req.method==="POST"&&req.url==="/action"){const buffers:Buffer[]=[];let bytes=0;try{for await(const buffer of req){bytes+=buffer.length;if(bytes>32768)throw new Error("large");buffers.push(buffer);}await action(Buffer.concat(buffers));actionCursor.finish("completed");res.writeHead(200);res.end("ok");}catch{res.writeHead(502);res.end("action_outcome_unknown");}return;}
 res.writeHead(404);res.end();
});server.listen(port,"127.0.0.1",()=>console.log("Private media bridge ready",deviceId,port));
process.on("SIGTERM",()=>{stopDesktop();stopScreen();hub.close();server.close();client?.close();process.exit(0);});
