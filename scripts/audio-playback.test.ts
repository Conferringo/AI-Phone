import {expect,test} from 'bun:test';
import {PcmDecoder,AudioPlayback,playbackVolume,applyPlaybackVolume} from '../apps/web/src/lib/audio-playback';

test('PCM keeps stereo frames split across arbitrary network boundaries',()=>{
 const bytes=new Uint8Array([0,128,255,127,0,64,0,192]);
 const decoder=new PcmDecoder();
 expect(decoder.decode(bytes.slice(0,3))).toBeNull();
 const first=decoder.decode(bytes.slice(3,5))!;
 expect([...first[0]]).toEqual([-1]);expect([...first[1]]).toEqual([32767/32768]);
 const second=decoder.decode(bytes.slice(5))!;
 expect([...second[0]]).toEqual([.5]);expect([...second[1]]).toEqual([-.5]);
});
test('new connection discards the preceding connection partial frame',()=>{
 const decoder=new PcmDecoder();decoder.decode(new Uint8Array([255,255]));decoder.reset();
 expect(decoder.decode(new Uint8Array([0,0,0,0]))?.[0][0]).toBe(0);
});
function fixture(){
 let state='suspended',stops=0,closes=0;const scheduled:number[]=[];const buffers:{channels:number,length:number,rate:number,samples:Record<number,number[]>}[]=[];const ramps:number[][]=[];const gain={gain:{value:1,setTargetAtTime(...args:number[]){ramps.push(args)}},connect(){},disconnect(){}};
 const context={currentTime:1,get state(){return state},destination:{},onstatechange:null,
 resume:async()=>{state='running'},suspend:async()=>{state='suspended'},close:async()=>{closes++;state='closed'},
 createGain:()=>gain,
 createBuffer:(channels:number,length:number,rate:number)=>{const record={channels,length,rate,samples:{} as Record<number,number[]>};buffers.push(record);return{duration:length/rate,copyToChannel(data:Float32Array,channel:number){record.samples[channel]=[...data]}}},
 createBufferSource:()=>({buffer:null,connect(){},disconnect(){},start(at:number){scheduled.push(at)},stop(){stops++},onended:null}),
 };
 return{context:context as unknown as AudioContext,scheduled,buffers,ramps,gain,setTime:(time:number)=>{context.currentTime=time},setState:(s:string)=>{state=s},stops:()=>stops,closes:()=>closes};
}
test('playback only reports running after AudioContext actually runs',async()=>{
 const f=fixture();f.context.resume=async()=>{};
 const player=new AudioPlayback(()=>f.context);
 await expect(player.resume()).rejects.toThrow('声音未开启');
 expect(()=>player.push(new Uint8Array(4))).toThrow();
});
test('endpoint reset clears scheduled audio without suspending an enabled context',async()=>{
 const f=fixture(),player=new AudioPlayback(()=>f.context);await player.resume();
 player.push(new Uint8Array(1920));expect(f.scheduled).toEqual([1.06]);player.reset();
 expect(f.stops()).toBe(1);expect(f.context.state).toBe('running');
 player.push(new Uint8Array(1920));expect(f.scheduled).toHaveLength(2);
 player.close();player.close();expect(f.closes()).toBe(1);
});
test('a stalled context never accumulates stale samples',async()=>{
 const f=fixture(),player=new AudioPlayback(()=>f.context);await player.resume();
 f.setState('suspended');expect(()=>player.push(new Uint8Array(9600))).toThrow('声音播放已暂停');
 expect(f.scheduled).toHaveLength(0);
});

test('volume reaches the device gain before and during playback, including mute',async()=>{
 expect(playbackVolume(-2)).toBe(0);expect(playbackVolume(3)).toBe(1);expect(playbackVolume(NaN)).toBe(0);
 expect(applyPlaybackVolume(null,.4,1)).toBe(.4);
 const f=fixture(),player=new AudioPlayback(()=>f.context);player.suspend();player.setVolume(.25);await player.resume();
 expect(f.gain.gain.value).toBe(.25);player.setVolume(0);expect(f.ramps.at(-1)).toEqual([0,1,.02]);
 player.suspend();expect(f.context.state).toBe('suspended');await player.resume();player.close();
});
test('sample values, both channels, and partial frames reach the graph intact',async()=>{
 const f=fixture(),player=new AudioPlayback(()=>f.context);await player.resume();
 player.push(new Uint8Array([0,32,0,224,0]));player.push(new Uint8Array([64,0,192,0,96,0,160]));
 expect(f.buffers).toEqual([
  {channels:2,length:1,rate:48000,samples:{0:[.25],1:[-.25]}},
  {channels:2,length:2,rate:48000,samples:{0:[.5,.75],1:[-.5,-.75]}},
 ]);
 expect(f.scheduled[1]).toBe(1.06+1/48000);player.close();
});
test('a delayed large packet is bounded to the latest 200ms',()=>{
 const data=new Uint8Array(48000);new DataView(data.buffer).setInt16(47996,16384,true);
 const decoded=new PcmDecoder().decode(data)!;
 expect(decoded[0].length).toBe(9600);expect(decoded[1].length).toBe(9600);expect(decoded[0][9599]).toBe(.5);
});
test('scheduler preserves continuity at the clock boundary and catches up after stalls',async()=>{
 const f=fixture(),player=new AudioPlayback(()=>f.context);await player.resume();
 player.push(new Uint8Array(1920));f.setTime(1.07);
 player.push(new Uint8Array(1920));expect(f.scheduled[1]).toBe(1.07);expect(f.stops()).toBe(0);
 f.setTime(2);player.push(new Uint8Array(1920));expect(f.scheduled[2]).toBe(2.06);expect(f.stops()).toBe(2);
 player.close();
});
test('queue latency stays bounded when transport delivers a burst',async()=>{
 const f=fixture(),player=new AudioPlayback(()=>f.context);await player.resume();
 for(let i=0;i<5;i++)player.push(new Uint8Array(38400));
 expect(f.stops()).toBeGreaterThan(0);expect(f.scheduled.at(-1)!-f.context.currentTime).toBeLessThan(.36);player.close();
});
test('reset never retains an arbitrary timestamp from an earlier connection',async()=>{
 const f=fixture(),player=new AudioPlayback(()=>f.context);f.setTime(.8);await player.resume();
 player.reset();player.push(new Uint8Array(4));expect(f.scheduled).toEqual([.8+.06]);player.close();
});
test('exactly 350ms queued audio does not introduce a needless discontinuity',async()=>{
 const f=fixture(),player=new AudioPlayback(()=>f.context);f.setTime(.01);await player.resume();
 player.push(new Uint8Array(38400));player.push(new Uint8Array(17280));player.push(new Uint8Array(4));
 expect(f.scheduled[2]).toBe(.36);expect(f.stops()).toBe(0);player.close();
});
