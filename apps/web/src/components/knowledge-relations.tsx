'use client';
import {useI18n} from "./locale-provider";
import {useEffect,useState} from 'react';
import type {MessageShape} from '@bufbuild/protobuf';
import {WorkbenchRequestSchema,KnowledgeListSchema,KnowledgeDocumentSchema,KnowledgeLinkRequestSchema} from '@ai-phone/protocol/ai_phone/v1/workbench_pb';
import {workbench} from '@/lib/workbench-client';
import {Plus,Search,Link2,ArrowUpRight,Check,X} from 'lucide-react';
import {Button} from './ui/button';
type Doc=MessageShape<typeof KnowledgeDocumentSchema>;
export function KnowledgeRelations({doc,disabled,onChanged,onSelect}:{doc:{id:string;revision:bigint;relatedIds:string[]};disabled:boolean;onChanged:(value:Doc)=>void;onSelect:(doc:Doc)=>void}){const {t}=useI18n();
 const [related,setRelated]=useState<Doc[]>([]),[options,setOptions]=useState<Doc[]>([]),[query,setQuery]=useState(''),[picking,setPicking]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState('');
 useEffect(()=>{let live=true;void workbench('knowledge-related',WorkbenchRequestSchema,{id:doc.id},KnowledgeListSchema).then(v=>{if(live)setRelated(v.documents);}).catch(e=>{if(live)setError(e.message);});return()=>{live=false;};},[doc.id,doc.revision]);
 useEffect(()=>{if(!picking)return;let live=true;const timer=setTimeout(()=>void workbench('knowledge',WorkbenchRequestSchema,{query},KnowledgeListSchema).then(v=>{if(live)setOptions(v.documents.filter(v=>v.id!==doc.id));}).catch(e=>{if(live)setError(e.message);}),150);return()=>{live=false;clearTimeout(timer);};},[picking,query,doc.id]);
 async function toggle(id:string){setBusy(true);setError('');try{const value=await workbench('knowledge-link',KnowledgeLinkRequestSchema,{id:doc.id,targetId:id,present:!doc.relatedIds.includes(id),expectedRevision:doc.revision},KnowledgeDocumentSchema);onChanged(value);}catch(e){setError(e instanceof Error?e.message:'关联失败');}finally{setBusy(false);}}
 return <section className="knowledge-related"><header className="knowledge-related-header"><h3>{t("关联文档")}</h3><Button variant="ghost" size="icon" aria-label={picking?t("收起关联选择"):t("关联文档")} disabled={disabled||busy} onClick={()=>setPicking(v=>!v)}>{picking?<X size={16}/>:<Plus size={16}/>}</Button></header>{error&&<p role="alert" className="workbench-error">{t(error)}</p>}
 <div className="knowledge-related-links">{related.map(v=><button key={v.id} type="button" disabled={disabled||busy} onClick={()=>onSelect(v)}><span>{v.title}</span><ArrowUpRight size={15}/></button>)}</div>
 {picking&&<div className="knowledge-link-picker"><label><Search size={16}/><input aria-label={t("搜索关联文档")} value={query} onChange={e=>setQuery(e.target.value)} placeholder={t("搜索文档")}/></label><div className="knowledge-link-options">{options.slice(0,30).map(v=><button type="button" key={v.id} aria-pressed={doc.relatedIds.includes(v.id)} disabled={busy||disabled} onClick={()=>void toggle(v.id)}><span>{v.title}</span>{doc.relatedIds.includes(v.id)?<Check size={16}/>:<Link2 size={15}/>}</button>)}{!options.length&&<p>{t("没有可关联的文档")}</p>}</div></div>}
 </section>;
}
