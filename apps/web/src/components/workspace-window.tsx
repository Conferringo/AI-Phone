"use client";
import {useI18n} from "./locale-provider";
import {useCallback,useEffect,useRef,useState} from "react";
import type {MessageShape} from "@bufbuild/protobuf";
import {WorkbenchRequestSchema,WorkspaceFilesSchema} from "@ai-phone/protocol/ai_phone/v1/workbench_pb";
import {workbench,uploadWorkbench} from "@/lib/workbench-client";
import {FloatingWindow} from "./floating-window";
import {Button} from "./ui/button";
import {Folder,File,Download,ArrowUp,Upload,LoaderCircle} from "lucide-react";
export function WorkspaceWindow({open,onClose}:{open:boolean;onClose:()=>void}){const {t}=useI18n();
 const [files,setFiles]=useState<MessageShape<typeof WorkspaceFilesSchema>["files"]>([]),[path,setPath]=useState(""),[error,setError]=useState(""),[loading,setLoading]=useState(true),[uploading,setUploading]=useState(false);
 const input=useRef<HTMLInputElement>(null),request=useRef(0),livePath=useRef(path),shown=useRef(open);livePath.current=path;shown.current=open;
 const load=useCallback(async(directory:string)=>{
  const version=++request.current;
  try{const result=await workbench("files",WorkbenchRequestSchema,{id:directory},WorkspaceFilesSchema);if(version===request.current&&livePath.current===directory&&shown.current){setFiles(result.files);setError("");}}
  catch(e){if(version===request.current&&shown.current)setError(e instanceof Error?e.message:"读取失败");}
  finally{if(version===request.current&&shown.current)setLoading(false);}
 },[]);
 useEffect(()=>{
  if(!open)return;setLoading(true);setFiles([]);void load(path);
  const timer=setInterval(()=>{if(!document.hidden)void load(path);},3000);
  return()=>{clearInterval(timer);request.current++;};
 },[open,path,load]);
 async function upload(file:globalThis.File){
  if(uploading)return;setUploading(true);setError("");
  try{await uploadWorkbench("upload-file",file,WorkspaceFilesSchema);if(livePath.current==="uploads")await load("uploads");else setPath("uploads");}
  catch(e){setError(e instanceof Error?e.message:"上传失败");}
  finally{setUploading(false);}
 }
 return <FloatingWindow fitContent id="workspace-files" title={t("工作文件")} open={open} onClose={onClose} width={650} height={500} actions={<Button variant="ghost" size="icon" aria-label={t("上传工作文件")} disabled={uploading} onClick={()=>input.current?.click()}>{uploading?<LoaderCircle className="spinning" size={17}/>:<Upload size={17}/>}</Button>}>
 <input ref={input} type="file" hidden onChange={e=>{const file=e.target.files?.[0];if(file)void upload(file);e.target.value="";}}/>
 {path&&<div className="file-path"><Button variant="ghost" size="icon" aria-label={t("上一级目录")} onClick={()=>setPath(path.split("/").slice(0,-1).join("/"))}><ArrowUp size={16}/></Button><span>{path.split("/").join(" / ")}</span></div>}
 {error&&<p className="workbench-error" role="alert">{t(error)}</p>}
 <div className="workspace-files" aria-busy={loading}>{loading?<div className="knowledge-empty" role="status"><LoaderCircle className="spinning" size={20}/><span className="sr-only">{t("加载文件")}</span></div>:files.length?files.map(file=><article key={file.path}>{file.directory?<Folder size={19}/>:<File size={19}/>} {file.directory?<button onClick={()=>setPath(file.path)}>{file.name}</button>:<a className="workspace-file-link" href={"/api/workbench/file?path="+encodeURIComponent(file.path)} download={file.name}><span>{file.name}</span><Download size={16}/></a>}<span>{file.directory?"":Number(file.sizeBytes)<1024?file.sizeBytes.toString()+" B":(Number(file.sizeBytes)/1024).toFixed(1)+" KB"}</span></article>):!error&&<div className="knowledge-empty">{t("暂无文件")}</div>}</div>
 </FloatingWindow>;
}
