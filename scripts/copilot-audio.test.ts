import {expect,test} from 'bun:test';
import {pcm16,wav,SpeechSegments} from '../apps/web/src/lib/copilot-audio';

test('actual PCM bytes resample to mono 16k WAV without losing duration or clipping',()=>{
  const a=pcm16(new Float32Array(4800).fill(.5),48000),b=pcm16(new Float32Array(4410).fill(-2),44100);
  expect(a.length).toBe(1600);expect(b.length).toBe(1600);expect(a[0]).toBe(16384);expect(b[0]).toBe(-32768);
  const bytes=wav([a,b]),view=new DataView(bytes.buffer);expect(new TextDecoder().decode(bytes.slice(0,4))).toBe('RIFF');
  expect(view.getUint32(24,true)).toBe(16000);expect(view.getUint32(40,true)).toBe(6400);expect(view.getInt16(44+3200,true)).toBe(-32768);
});
test('speech pins its initial device, streams phrase drafts and ends at 1500ms of silence',()=>{
  let current='phone';const events:Array<{kind:string;target:string;at?:number;final?:boolean}>=[];
  const segments=new SpeechSegments({choose:()=>current,start:(id,target,at)=>events.push({kind:'start',target,at}),speech:()=>{},chunk:(id,target,frames,final)=>{expect(wav(frames).length).toBeLessThanOrEqual(960044);events.push({kind:'chunk',target,final});},end:(id,target,at)=>events.push({kind:'end',target,at})},()=> 'utterance');
  const speech=new Int16Array(800).fill(3000),quiet=new Int16Array(800);
  for(let at=0;at<2100;at+=50){segments.push(speech,at);if(at===1000)current='desktop';}
  for(let at=2100;at<=3550;at+=50)segments.push(quiet,at);
  expect(events[0]).toMatchObject({kind:'start',target:'phone'});expect(events.some(v=>v.kind==='chunk'&&!v.final)).toBe(true);
  expect(events.at(-1)).toEqual({kind:'end',target:'phone',at:3550});
  segments.push(speech,3600);segments.push(speech,3650);expect(events.at(-1)).toMatchObject({kind:'start',target:'desktop'});
});
test('short clicks and quiet background do not start a model request',()=>{
  const events:string[]=[];const s=new SpeechSegments({choose:()=> 'phone',start:()=>events.push('start'),speech:()=>{},chunk:()=>events.push('chunk'),end:()=>events.push('end')});
  s.push(new Int16Array(800).fill(3000),0);for(let at=50;at<3000;at+=50)s.push(new Int16Array(800),at);expect(events).toEqual([]);
});
