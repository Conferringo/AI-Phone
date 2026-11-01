import {toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ChatMessageSchema,ChatWindowKind,type ChatStateSchema} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
import type {ChatSnapshot,HistoryBoundary,SnapshotVersion} from './chat-snapshot';
type Chat=MessageShape<typeof ChatStateSchema>;
type Message=Chat['messages'][number];
export const HISTORY_LIMIT=40;

export function historyRequest(conversationId:string,deviceId:string,chat:Chat|null=null,receiptRequestIds:string[]=[],beforeMessageId='',watchOffset=0){
 const active=[...new Set(chat?.messages.filter(message=>message.role==='assistant'&&message.active).map(message=>message.id)??[])],count=Math.min(8,active.length);
 const watchMessageIds=Array.from({length:count},(_,index)=>active[(active.length-count+watchOffset+index)%active.length]);
 return{conversationId,deviceId,limit:HISTORY_LIMIT,beforeMessageId,watchMessageIds,receiptRequestIds};
}

/** Rotate over the existing accepted queue; never make a second receipt store. */
export function receiptBatch(entries:ReadonlyArray<{id:string;status:string}>,offset:number){
 const ids=entries.filter(item=>item.status==='accepted').map(item=>item.id);
 if(!ids.length)return{ids:[],next:0};
 const start=offset%ids.length,count=Math.min(32,ids.length);
 return{ids:Array.from({length:count},(_,index)=>ids[(start+index)%ids.length]),next:(start+count)%ids.length};
}

function sameMessage(a:Message,b:Message){
 const left=toBinary(ChatMessageSchema,a),right=toBinary(ChatMessageSchema,b);
 return left.length===right.length&&left.every((byte,index)=>byte===right[index]);
}

function messageVersions(previous:ChatSnapshot){
 if(previous.messageVersions)return new Map(previous.messageVersions);
 return new Map(previous.chat?.messages.map(message=>[message.id,{revision:previous.chat!.revision,order:previous.order}] as const)??[]);
}

function isOlder(next:SnapshotVersion,known:SnapshotVersion){return next.revision<known.revision||next.revision===known.revision&&next.order<known.order;}

function upsert(messages:Map<string,Message>,versions:Map<string,SnapshotVersion>,incoming:Message,version:SnapshotVersion,minimumRevision:bigint){
 const known=messages.get(incoming.id);
 if(known&&(version.revision<minimumRevision||isOlder(version,versions.get(incoming.id)!)))return;
 messages.set(incoming.id,known&&sameMessage(known,incoming)?known:incoming);
 versions.set(incoming.id,version);
}

function byOrdinal(a:Message,b:Message){return a.ordinal<b.ordinal?-1:a.ordinal>b.ordinal?1:0;}

function mergeMessages(previous:ChatSnapshot,next:Chat,order:number){
 const messages=new Map(previous.chat?.messages.map(message=>[message.id,message])??[]),versions=messageVersions(previous);
 for(const incoming of next.messages)upsert(messages,versions,incoming,{revision:next.revision,order},previous.chat?.revision??0n);
 return{messages:[...messages.values()].sort(byOrdinal),versions};
}

function emptyBoundary(known:HistoryBoundary|undefined,next:Chat,order:number,beforeMessageId:string){
 if(known&&next.messageWindow!.kind===ChatWindowKind.BEFORE&&beforeMessageId===known.beforeMessageId&&order>=known.order)return{...known,hasOlder:next.messageWindow!.hasOlder,order};
 return known;
}

function nextBoundary(previous:ChatSnapshot,next:Chat,order:number,beforeMessageId:string):HistoryBoundary|undefined{
 const window=next.messageWindow!,known=previous.history;
 const oldest=next.messages.find(message=>message.id===window.nextBeforeMessageId);
 if(!oldest)return emptyBoundary(known,next,order,beforeMessageId);
 if(known&&oldest.ordinal>known.ordinal)return known;
 if(known&&oldest.ordinal===known.ordinal&&order<known.order)return known;
 return{beforeMessageId:oldest.id,ordinal:oldest.ordinal,hasOlder:window.hasOlder,order};
}

function checkScope(previous:ChatSnapshot,next:Chat){
 if(previous.chat&&(previous.chat.conversationId!==next.conversationId||previous.chat.deviceId!==next.deviceId))throw Error('对话状态与当前设备不一致');
}

function nextHeader(previous:ChatSnapshot,next:Chat,order:number){
 return previous.chat&&isOlder({revision:next.revision,order},{revision:previous.chat.revision,order:previous.headerOrder??previous.order})?previous.chat:next;
}

function newestRevision(previous:ChatSnapshot,next:Chat){return previous.chat&&previous.chat.revision>next.revision?previous.chat.revision:next.revision;}

export function mergeMessageWindow(previous:ChatSnapshot,next:Chat,order:number,beforeMessageId=''):ChatSnapshot{
 checkScope(previous,next);
 const kind=next.messageWindow?.kind;
 if(kind!==ChatWindowKind.TAIL&&kind!==ChatWindowKind.BEFORE)throw Error('服务返回了无效的消息窗口');
 const merged=mergeMessages(previous,next,order),headerOrder=previous.headerOrder??previous.order;
 const header=nextHeader(previous,next,order),revision=newestRevision(previous,next);
 return{chat:{...header,messages:merged.messages,revision},order:Math.max(previous.order,order),headerOrder:header===next?order:headerOrder,messageVersions:merged.versions,history:nextBoundary(previous,next,order,beforeMessageId)};
}
