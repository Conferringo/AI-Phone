import type {MessageInitShape} from '@bufbuild/protobuf';
import type {DesktopActionRequestSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';

type Input=MessageInitShape<typeof DesktopActionRequestSchema>;
type Size={bytes:number;characters:number};
type Pending={input:Input;size:Size|null};
const MAX_TEXT_BYTES=64_000,MAX_TEXT_CHARACTERS=16_000;
const encoder=new TextEncoder();

function textSize(text:string):Size {
 let characters=0;
 for(const _ of text)characters++;
 return {bytes:encoder.encode(text).byteLength,characters};
}

/** Coalesce only an undispatched text tail; each dispatched action remains immutable. */
export class DesktopInputQueue {
 private work=Promise.resolve();
 private pending:Pending|null=null;

 barrier(){this.pending=null;}

 enqueue(input:Input,dispatch:(input:Input)=>Promise<void>):Promise<void> {
  const size=input.kind==='text'?textSize(input.text??''):null;
  const last=this.pending;
  if(last?.size&&size&&
   last.size.bytes+size.bytes<=MAX_TEXT_BYTES&&
   last.size.characters+size.characters<=MAX_TEXT_CHARACTERS){
   last.input.text=(last.input.text??'')+(input.text??'');
   last.size.bytes+=size.bytes;
   last.size.characters+=size.characters;
   return this.work;
  }
  const entry:Pending={input:{...input},size};
  this.pending=entry;
  // Callers observe this promise's rejection. A later independent input may
  // continue through its existing fresh-observation path; it never retries it.
  this.work=this.work.catch(()=>{}).then(()=>{
   if(this.pending===entry)this.pending=null;
   return dispatch(Object.freeze(entry.input));
  });
  return this.work;
 }
}
