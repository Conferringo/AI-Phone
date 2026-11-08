"use client";
import {useI18n} from "./locale-provider";
import {useEffect,useRef,useState} from "react";
import type {MessageShape} from "@bufbuild/protobuf";
import {WorkbenchRequestSchema,KnowledgeListSchema,KnowledgeDocumentSchema,KnowledgeSaveRequestSchema,KnowledgeDeleteRequestSchema} from "@ai-phone/protocol/ai_phone/v1/workbench_pb";
import {workbench} from "@/lib/workbench-client";
import {KnowledgeRelations} from './knowledge-relations';
import {KnowledgeNetwork} from "./knowledge-network";
import {FloatingWindow} from "./floating-window";
import {Button} from "./ui/button";
import {Plus,Search,PenLine,Save,Trash2} from "lucide-react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
type Doc=MessageShape<typeof KnowledgeDocumentSchema>;
type Draft={id:string;title:string;content:string;revision:bigint;relatedIds:string[];dirty?:boolean};
export function KnowledgeWindow({open,onClose}:{open:boolean;onClose:()=>void}){const {t}=useI18n();
 const [docs,setDocs]=useState<Doc[]>([]),[query,setQuery]=useState(""),[error,setError]=useState(""),[busy,setBusy]=useState(false),[selected,setSelected]=useState(""),[editorOpen,setEditorOpen]=useState(false),[editing,setEditing]=useState(false),[drafts,setDrafts]=useState<Record<string,Draft>>({});
 const selection=useRef("");selection.current=selected;const draft=drafts[selected];
 async function load(){try{setDocs((await workbench("knowledge",WorkbenchRequestSchema,{query},KnowledgeListSchema)).documents);}catch(e){setError(e instanceof Error?e.message:"读取失败");}}
 useEffect(()=>{if(!open)return;const timer=setTimeout(()=>void load(),query?180:0);const interval=setInterval(()=>void load(),4000);return()=>{clearTimeout(timer);clearInterval(interval);};},[open,query]);
 async function select(doc:Doc){setSelected(doc.id);setEditorOpen(true);if(drafts[doc.id]?.dirty)return;try{const value=await workbench("knowledge-read",WorkbenchRequestSchema,{id:doc.id},KnowledgeDocumentSchema);setDrafts(v=>({...v,[value.id]:{id:value.id,title:value.title,content:value.content,revision:value.revision,relatedIds:value.relatedIds}}));if(selection.current===doc.id)setEditing(false);}catch(e){setError(e instanceof Error?e.message:"读取失败");}}
 function create(){if(draft&&!draft.id){setEditing(true);setEditorOpen(true);return;}const key="draft-"+Date.now();setDrafts(v=>({...v,[key]:{id:"",title:"",content:"",revision:0n,relatedIds:[]}}));setSelected(key);setEditing(true);setEditorOpen(true);}
 function change(value:Partial<Draft>){setDrafts(v=>({...v,[selected]:{...v[selected],...value,dirty:true}}));}
 async function save(){if(!draft||busy)return;setBusy(true);setError("");const key=selected;try{const value=await workbench("knowledge-save",KnowledgeSaveRequestSchema,{id:draft.id,title:draft.title,content:draft.content,expectedRevision:draft.revision,relatedIds:draft.relatedIds},KnowledgeDocumentSchema);setDrafts(v=>({...v,[value.id]:{id:value.id,title:value.title,content:value.content,revision:value.revision,relatedIds:value.relatedIds}}));if(selection.current===key){setSelected(value.id);setEditing(false);}await load();}catch(e){setError(e instanceof Error?e.message:"保存失败");}finally{setBusy(false);}}
 async function trash(doc:Pick<Doc,"id"|"revision">){setBusy(true);try{await workbench("knowledge-delete",KnowledgeDeleteRequestSchema,{id:doc.id,expectedRevision:doc.revision},KnowledgeListSchema);if(selected===doc.id)setEditorOpen(false);await load();}catch(e){setError(e instanceof Error?e.message:"移除失败");}finally{setBusy(false);}}
 return <>
 <FloatingWindow id="knowledge" title={t("知识库")} open={open} onClose={onClose} width={850} height={590} actions={<Button variant="ghost" size="icon" aria-label={t("新建知识文档")} onClick={create}><Plus size={18}/></Button>}>
 <div className="knowledge-search"><Search size={17}/><input aria-label={t("搜索知识库")} placeholder={t("搜索")} value={query} onChange={e=>setQuery(e.target.value)}/>{busy&&<span role="status">{t("处理中…")}</span>}</div>
 {error&&<div className="workbench-error" role="alert">{t(error)}</div>}
 <div className="knowledge-view">
 {!docs.length?<div className="knowledge-empty"><p>{query?t("没有匹配的文档"):t("知识库为空")}</p>{!query&&<Button variant="outline" onClick={create}><Plus size={16}/>{t("新建文档")}</Button>}</div>:<KnowledgeNetwork docs={docs} selected={selected} onSelect={doc=>void select(doc as Doc)}/>}
 </div></FloatingWindow>
 <FloatingWindow id="knowledge-document" parentId="knowledge" anchorKey={selected} title={draft?.title||t("新建文档")} open={open&&editorOpen} onClose={()=>setEditorOpen(false)} width={700} height={600} actions={draft&&<>{!editing&&<Button variant="ghost" size="icon" aria-label={t("编辑文档")} onClick={()=>setEditing(true)}><PenLine size={17}/></Button>}{editing&&<><Button variant="ghost" size="icon" aria-label={t("保存文档")} disabled={busy||!draft.title.trim()||(!draft.dirty&&!!draft.id)} onClick={()=>void save()}><Save size={17}/></Button></>}{draft.id&&<Button variant="ghost" size="icon" aria-label={t("移除当前文档")} disabled={busy} onClick={()=>void trash({id:draft.id,revision:draft.revision})}><Trash2 size={16}/></Button>}</>}>
 {draft&&<div className="knowledge-document">{error&&<p className="workbench-error" role="alert">{t(error)}</p>}
 {editing?<><input disabled={busy} className="knowledge-title-input" aria-label={t("文档标题")} placeholder={t("标题")} value={draft.title} onChange={e=>change({title:e.target.value})}/><textarea disabled={busy} className="knowledge-content-input" aria-label={t("文档正文")} placeholder={t("正文")} value={draft.content} onChange={e=>change({content:e.target.value})} onKeyDown={e=>{if(e.key==="Enter"&&(e.metaKey||e.ctrlKey)&&!e.nativeEvent.isComposing){e.preventDefault();void save();}}}/></>:<div className="markdown-document">{!draft.content.trim()&&<div className="knowledge-empty"><Button variant="ghost" onClick={()=>setEditing(true)}><PenLine size={16}/>{t("编辑文档")}</Button></div>}<Markdown remarkPlugins={[remarkGfm]}>{draft.content}</Markdown></div>}
 {draft.id&&<KnowledgeRelations key={draft.id} doc={draft} disabled={editing||busy} onSelect={d=>void select(d)} onChanged={value=>{setDrafts(v=>({...v,[value.id]:{id:value.id,title:value.title,content:value.content,revision:value.revision,relatedIds:value.relatedIds}}));void load();}}/>}
 </div>}
 </FloatingWindow></>;
}
