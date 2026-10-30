import {beginAudio} from './audio-transport';
import type {ServerResponse} from 'node:http';
import {guestCapture} from './guest-capture';
type Capture=ReturnType<typeof guestCapture>;
import {ScreenHub} from './screen-hub';
let capture:Capture|null=null;
const hub=new ScreenHub(async frame=>frame.image,stop,1000/12);
function stop(){const old=capture;capture=null;old?.stop();hub.advance();}
export function desktopScreen(res:ServerResponse){
 res.writeHead(200,{'content-type':'multipart/x-mixed-replace; boundary=phone','cache-control':'private,no-store'});res.flushHeaders();hub.add(res);
 if(capture)return;
 const owned=guestCapture('screen'),child=owned.child;capture=owned;let buffer=Buffer.alloc(0);
 child.stdout!.on('data',(part:Buffer)=>{if(capture!==owned)return;buffer=Buffer.concat([buffer,part]);if(buffer.length>4*1024*1024){stop();hub.close();return;}for(;;){const begin=buffer.indexOf(Buffer.from([255,216]));if(begin<0){buffer=Buffer.alloc(0);break;}const end=buffer.indexOf(Buffer.from([255,217]),begin+2);if(end<0)break;const jpeg=Buffer.from(buffer.subarray(begin,end+2));buffer=buffer.subarray(end+2);hub.offer({format:{width:1280,height:800},image:jpeg,timestampUs:Date.now()*1000},0);}});
 child.stderr!.on('data',()=>{});const end=()=>{if(capture===owned){capture=null;hub.close();}};child.on('error',end);child.on('exit',end);
}
const sweep=setInterval(()=>hub.sweep(),1000);sweep.unref();
const audioCaptures=new Set<Capture>();
export function desktopAudio(res:ServerResponse){
 const owned=guestCapture('audio'),child=owned.child;
 audioCaptures.add(owned);
 beginAudio(res);
 child.stdout!.on('data',(data:Buffer)=>{if(res.writableLength>192000){res.destroy();return;}res.write(data);});
 child.stderr!.on('data',()=>{});
 const end=()=>{audioCaptures.delete(owned);res.end();};child.on('error',end);child.on('exit',end);
 res.on('close',()=>{owned.stop();audioCaptures.delete(owned);});
}
export function stopDesktop(){stop();hub.close();for(const owned of audioCaptures)owned.stop();audioCaptures.clear();}
