import {expect,test} from 'bun:test';
import {WaveDecoder,playWave} from '../apps/web/src/lib/copilot-playback';
import {wav} from '../apps/web/src/lib/copilot-audio';

test('WAV headers and PCM samples can cross every network byte boundary',()=>{
  const bytes=wav([new Int16Array([-32768,0,32767])]),decoder=new WaveDecoder(),samples:number[]=[];
  for(const byte of bytes)for(const frame of decoder.push(new Uint8Array([byte])))samples.push(...frame.channels[0]);
  expect(samples).toEqual([-1,0,32767/32768]);expect(decoder.finish()).toBeNull();
});
test('streaming WAV unknown lengths work, but truncation and malformed headers fail',()=>{
  const bytes=wav([new Int16Array([100,200])]);new DataView(bytes.buffer).setUint32(40,0xffffffff,true);
  const stream=new WaveDecoder();expect(stream.push(bytes)[0].channels[0]).toHaveLength(2);expect(stream.finish()).toBeNull();
  const truncated=new WaveDecoder();truncated.push(wav([new Int16Array([1,2])]).slice(0,-1));expect(()=>truncated.finish()).toThrow();
  expect(()=>new WaveDecoder().push(new Uint8Array(44))).toThrow();
  for(const at of [0,8]){const invalid=bytes.slice();invalid[at]=0;expect(()=>new WaveDecoder().push(invalid)).toThrow();}
  const unaligned=bytes.slice();new DataView(unaligned.buffer).setUint32(40,3,true);expect(()=>new WaveDecoder().push(unaligned)).toThrow();
  const unknownOdd=new WaveDecoder();unknownOdd.push(bytes.slice(0,-1));expect(()=>unknownOdd.finish()).toThrow();
});
test('non-PCM WAV retains its original container for the existing browser decoder',()=>{
  const bytes=wav([new Int16Array(4)]),view=new DataView(bytes.buffer);
  view.setUint16(20,3,true);view.setUint16(32,4,true);view.setUint16(34,32,true);view.setUint32(28,64000,true);view.setFloat32(44,.25,true);view.setFloat32(48,-.25,true);
  const decoder=new WaveDecoder();expect(decoder.push(bytes.slice(0,45))).toEqual([]);expect(decoder.push(bytes.slice(45))).toEqual([]);expect(decoder.finish()).toEqual(bytes);
});
test('playback schedules real samples before the provider finishes and abort stops queued sources',async()=>{
  let finish!:()=>void;const end=new Promise<void>(resolve=>{finish=resolve;});
  const sources:Array<{started:boolean;stopped:boolean;onended:(()=>void)|null}>=[];
  const context={currentTime:0,destination:{},createBuffer:(_channels:number,length:number,rate:number)=>({duration:length/rate,copyToChannel:()=>{}}),createBufferSource:()=>{
    const node={started:false,stopped:false,onended:null as (()=>void)|null,buffer:null,connect:()=>{},disconnect:()=>{},start:()=>{node.started=true;},stop:()=>{node.stopped=true;node.onended?.();}};sources.push(node);return node;
  }} as unknown as AudioContext;
  const control=new AbortController();
  async function* chunks(){yield wav([new Int16Array(1600)]);await end;}
  const playing=playWave(context,chunks(),control.signal);await Bun.sleep(0);
  expect(sources).toHaveLength(1);expect(sources[0].started).toBe(true);
  control.abort();expect(sources[0].stopped).toBe(true);finish();await playing;
});
