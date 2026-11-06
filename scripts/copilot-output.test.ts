import {expect,test} from 'bun:test';
import {CopilotOutput} from '../apps/web/src/lib/copilot-output';
const tick=()=>new Promise(resolve=>setTimeout(resolve,0));
test('incremental replies speak once and scrolling older history cannot become a new reply',async()=>{
  const spoken:string[]=[],speaker=new CopilotOutput(async(_target:string,text)=>{spoken.push(text);},message=>{throw Error(message);});
  const old={id:'old',text:'历史消息。',ordinal:10n,complete:true};speaker.feed('phone','agent',[old]);speaker.release();
  const fresh={id:'fresh',text:'正在处理。',ordinal:11n,complete:false};speaker.feed('phone','agent',[old,fresh]);await tick();
  for(let i=0;i<3;i++){speaker.feed('phone','agent',[old,{...fresh,complete:true}]);await tick();}
  speaker.feed('phone','agent',[{id:'older',text:'更早的历史。',ordinal:1n,complete:true},old,{...fresh,text:'正在处理。已经保存。',complete:true}]);await tick();
  expect(spoken).toEqual(['正在处理。','已经保存。']);speaker.close();
});
test('barge-in aborts audible output immediately and never replays the interrupted prefix',async()=>{
  const spoken:string[]=[],signals:AbortSignal[]=[];
  const speaker=new CopilotOutput(async(_target:string,text,signal)=>{spoken.push(text);signals.push(signal);await new Promise<void>(resolve=>signal.addEventListener('abort',()=>resolve(),{once:true}));},message=>{throw Error(message);});
  speaker.feed('phone','agent',[]);speaker.release();
  speaker.feed('phone','agent',[{id:'a',text:'这是之前的回复。',ordinal:1n,complete:false}]);await tick();
  speaker.hold();expect(signals[0].aborted).toBe(true);await tick();
  speaker.feed('phone','agent',[{id:'a',text:'这是之前的回复。',ordinal:1n,complete:true}]);speaker.release();await tick();
  expect(spoken).toEqual(['这是之前的回复。']);speaker.close();
});
test('an interrupted reply stays silent as late text arrives, while a new reply can speak',async()=>{
  const spoken:string[]=[],signals:AbortSignal[]=[];
  const speaker=new CopilotOutput(async(_target:string,text,signal)=>{
    spoken.push(text);signals.push(signal);
    if(spoken.length===1)await new Promise<void>(resolve=>signal.addEventListener('abort',()=>resolve(),{once:true}));
  },message=>{throw Error(message);});
  const previous={id:'previous',ordinal:1n,text:'先打开上海天气。',complete:false};
  try{
    speaker.feed('phone','agent',[]);speaker.release();
    speaker.feed('phone','agent',[previous]);await tick();
    speaker.hold();expect(signals[0].aborted).toBe(true);await tick();
    const late={...previous,text:previous.text+'正在打开上海。'};
    speaker.feed('phone','agent',[late]);speaker.release();await tick();
    const revised={...late,text:'上海天气已经打开。',complete:true};
    speaker.feed('phone','agent',[revised]);await tick();
    speaker.feed('phone','agent',[{...revised,text:revised.text+'旧回复的迟到尾句。'},{id:'current',ordinal:2n,text:'改为北京。',complete:true}]);await tick();
    expect(spoken).toEqual(['先打开上海天气。','改为北京。']);
  }finally{speaker.close();}
});
test('failed synthesis waits for explicit retry instead of looping or losing the sentence',async()=>{
  let failed=true;const spoken:string[]=[],errors:string[]=[];
  const speaker=new CopilotOutput(async(_target:string,text)=>{spoken.push(text);if(failed)throw Error('quota');},message=>errors.push(message));
  speaker.feed('scope','agent',[]);speaker.release();const reply={id:'a',text:'正在处理。',ordinal:1n,complete:true};
  speaker.feed('scope','agent',[reply]);await tick();speaker.feed('scope','agent',[reply]);await tick();
  expect(spoken).toHaveLength(1);expect(errors).toEqual(['quota']);
  failed=false;speaker.release();await tick();expect(spoken).toEqual(['正在处理。','正在处理。']);speaker.close();
});
