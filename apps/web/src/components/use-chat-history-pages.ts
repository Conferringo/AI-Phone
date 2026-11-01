'use client';
import {useCallback,useEffect,useRef,useState,type RefObject} from 'react';
import type {ChatSnapshot} from '../lib/chat-snapshot';
type Reader=(signal?:AbortSignal,beforeMessageId?:string)=>Promise<void>;
type PageOwner=RefObject<AbortController|null>;

async function readPage(owner:PageOwner,before:string,reader:Reader,loading:(value:boolean)=>void){
 if(!before||owner.current)return;
 const abort=new AbortController();owner.current=abort;loading(true);
 try{await reader(abort.signal,before);}
 finally{
  if(owner.current===abort)owner.current=null;
  if(!abort.signal.aborted)loading(false);
 }
}

function viewportAnchor(element:HTMLDivElement,snapshot:ChatSnapshot){
 if(!snapshot.history||!snapshot.chat)return '';
 const box=element.getBoundingClientRect();
 const row=document.elementFromPoint(box.left+Math.min(40,box.width/2),box.bottom-24)?.closest<HTMLElement>('[data-message-id]');
 const index=snapshot.chat.messages.findIndex(message=>message.id===row?.dataset.messageId);
 if(index<0)return '';
 return snapshot.chat.messages[index+1]?.id||'';
}

function refreshViewport(element:HTMLDivElement,snapshot:ChatSnapshot,last:RefObject<{anchor:string;at:number}>,read:(before:string)=>void){
 const anchor=viewportAnchor(element,snapshot);
 if(!anchor)return;
 if(last.current.anchor===anchor&&Date.now()-last.current.at<3000)return;
 last.current={anchor,at:Date.now()};read(anchor);
}

/** Owns only page reads, independent of focus and the background tail poll. */
export function useChatHistoryPages(reader:Reader,snapshot:ChatSnapshot){
 const owner=useRef<AbortController|null>(null),last=useRef({anchor:'',at:0});
 const [loading,setLoading]=useState(false);
 useEffect(()=>()=>owner.current?.abort(),[]);
 const read=useCallback((before:string)=>{void readPage(owner,before,reader,setLoading);},[reader]);
 return{loading,more:()=>read(snapshot.history?.beforeMessageId||''),revalidate:(element:HTMLDivElement)=>refreshViewport(element,snapshot,last,read)};
}
