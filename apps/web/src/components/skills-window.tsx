"use client";
import {useI18n} from "./locale-provider";
import {useEffect,useRef,useState} from "react";
import type {MessageShape} from "@bufbuild/protobuf";
import {WorkbenchRequestSchema,RuntimeSkillSchema,SkillListSchema,SkillImportRequestSchema,SkillUpdateRequestSchema} from "@ai-phone/protocol/ai_phone/v1/workbench_pb";
import {workbench} from "@/lib/workbench-client";
import {FloatingWindow} from "./floating-window";
import {Button} from "./ui/button";
import {Plus,Upload,Trash2,ChevronDown,Link,Undo2} from "lucide-react";
type Skill=MessageShape<typeof RuntimeSkillSchema>;
export function SkillsWindow({open,onClose}:{open:boolean;onClose:()=>void}){const {t}=useI18n();
 const [skills,setSkills]=useState<Skill[]>([]),[source,setSource]=useState(""),[adding,setAdding]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(""),[removed,setRemoved]=useState(""),[expanded,setExpanded]=useState("");const input=useRef<HTMLInputElement>(null);
 async function load(){try{setSkills((await workbench("skills",WorkbenchRequestSchema,{},SkillListSchema)).skills);setError("");}catch(e){setError(e instanceof Error?e.message:"读取失败");}}
 useEffect(()=>{if(open)void load();},[open]);
 async function update(id:string,enabled:boolean,remove=false){setBusy(true);try{setSkills((await workbench("skill-update",SkillUpdateRequestSchema,{id,enabled,remove},SkillListSchema)).skills);if(remove)setRemoved(id);else if(id===removed)setRemoved("");setError("");}catch(e){setError(e instanceof Error?e.message:"更新失败");}finally{setBusy(false);}}
 async function install(content="",name=""){if(busy)return;setBusy(true);setError("");try{await workbench("skill-import",SkillImportRequestSchema,{source:content?"":source,content,name},RuntimeSkillSchema);setSource("");setAdding(false);await load();}catch(e){setError(e instanceof Error?e.message:"导入失败");}finally{setBusy(false);}}
 return <FloatingWindow fitContent id="skills" title={t("Skill 管理")} open={open} onClose={onClose} width={650} height={560} actions={<><Button variant="ghost" size="icon" aria-label={t("上传Skill")} disabled={busy} onClick={()=>input.current?.click()}><Upload size={17}/></Button><Button variant="ghost" size="icon" aria-label={t("添加Skill")} aria-expanded={adding} onClick={()=>setAdding(v=>!v)}><Plus size={18}/></Button></>}>
 <input ref={input} type="file" accept=".md,text/markdown,text/plain" hidden onChange={async e=>{const file=e.target.files?.[0];if(file)await install(await file.text(),file.name.replace(/\.md$/,""));e.target.value="";}}/>
 {adding&&<form className="skill-import" onSubmit={e=>{e.preventDefault();void install();}}><Link size={17}/><input aria-label={t("Skill网址")} placeholder={t("GitHub / SKILL.md 网址")} type="url" required value={source} onChange={e=>setSource(e.target.value)} disabled={busy}/><Button disabled={busy} type="submit">{busy?t("导入中…"):t("导入")}</Button></form>}
 {error&&<p className="workbench-error" role="alert">{t(error)}</p>}
 <div className="skill-list">{skills.map(skill=><article className="skill-row" key={skill.id}><div className="skill-row-main"><button className="skill-disclosure" aria-expanded={expanded===skill.id} onClick={()=>setExpanded(v=>v===skill.id?"":skill.id)}><ChevronDown size={15}/><span>{skill.name}</span></button><div className="skill-row-controls">{!skill.builtin&&<><label className="skill-switch"><input type="checkbox" checked={skill.enabled} disabled={busy} aria-label={t("启用 {0}", {"0":skill.name})} onChange={e=>void update(skill.id,e.target.checked)}/></label><Button variant="ghost" size="icon" aria-label={t("移除 {0}", {"0":skill.name})} disabled={busy} onClick={()=>void update(skill.id,false,true)}><Trash2 size={15}/></Button></>}{skill.builtin&&<span className="skill-origin">{t("内置")}</span>}</div></div>{expanded===skill.id&&<div className="skill-expanded">{skill.description&&<p>{skill.description}</p>}{skill.source!=="builtin"&&<a href={skill.source.startsWith("https://")?skill.source:undefined} target="_blank" rel="noreferrer">{skill.source}</a>}<pre>{skill.content}</pre></div>}</article>)}</div>
 {removed&&<button className="undo-action" onClick={()=>void update(removed,true)}><Undo2 size={15}/>{t("撤销移除")}</button>}
 </FloatingWindow>;
}
