import type {MessageInitShape} from '@bufbuild/protobuf';
import type {DesktopActionRequestSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';

type Input=MessageInitShape<typeof DesktopActionRequestSchema>;
type Size={bytes:number;characters:number};
type Pending={input:Input;scope:string;size:Size};
const MAX_TEXT_BYTES=64_000,MAX_TEXT_CHARACTERS=16_000;
const textSize=(text:string):Size=>({bytes:new TextEncoder().encode(text).byteLength,characters:[...text].length});
const fits=(first:Size,next:Size)=>first.bytes+next.bytes<=MAX_TEXT_BYTES&&first.characters+next.characters<=MAX_TEXT_CHARACTERS;

/** Coalesce only an undispatched text tail; each dispatched action remains immutable. */
export class DesktopInputQueue {
 private work=Promise.resolve();
 private pending:Pending|null=null;

 barrier(){this.pending=null;}

 private merge(input:Input,scope:string){
  const last=this.pending;
  if(!last||last.scope!==scope)return false;
  if(last.input.kind!=='text'||input.kind!=='text')return false;
  const size=textSize(input.text??'');
  if(!fits(last.size,size))return false;
  last.input={...last.input,text:(last.input.text??'')+(input.text??'')};
  last.size={bytes:last.size.bytes+size.bytes,characters:last.size.characters+size.characters};
  return true;
 }

 enqueue(input:Input,scope:string,dispatch:(input:Input)=>Promise<void>):Promise<void> {
  if(this.merge(input,scope))return this.work;
  const entry:Pending={input:{...input},scope,size:input.kind==='text'?textSize(input.text??''):{bytes:0,characters:0}};
  this.pending=entry;
  // Callers observe this promise's rejection. A later independent input may
  // continue through its existing fresh-observation path; it never retries it.
  this.work=this.work.catch(()=>{}).then(()=>{
   if(this.pending===entry)this.pending=null;
   return dispatch(Object.freeze({...entry.input}));
  });
  return this.work;
 }
}
