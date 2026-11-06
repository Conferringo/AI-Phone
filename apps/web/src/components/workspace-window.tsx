'use client';
import {useEffect,useRef,useState,type DragEvent,type KeyboardEvent} from 'react';
import type {MessageInitShape,MessageShape} from '@bufbuild/protobuf';
import {FileLocationSchema,FileListingSchema,FileManageRequestSchema,FileManageResultSchema,FileOperation} from '@ai-phone/protocol/ai_phone/v1/workbench_pb';
import {ArrowUp,Check,Copy,Scissors,ClipboardPaste,FolderPlus,Folder,File,Download,Upload,Trash2,Pencil,RefreshCw,LoaderCircle,Smartphone,Monitor,X} from 'lucide-react';
import {fileRpc,uploadFile} from '@/lib/file-manager-client';
import {clientId} from '@/lib/client-id';
import {FloatingWindow} from './floating-window';
import {Button} from './ui/button';
import {useI18n} from './locale-provider';
import type {Registry} from './registry-window';
import './file-manager.css';

type Location=MessageInitShape<typeof FileLocationSchema>;
type Entry=MessageShape<typeof FileListingSchema>['entries'][number];
type Clipboard={sources:Location[];operation:FileOperation};
type Edit={kind:'mkdir'|'rename'|'delete';pane:string;location:Location;sources:Location[];value:string};
const MIME='application/x-ai-phone-files';
const parent=(path:string)=>path.slice(0,path.lastIndexOf('/'))||'/';
const bytes=(value:bigint)=>{const n=Number(value);return n<1024?`${n} B`:n<1048576?`${(n/1024).toFixed(1)} KB`:n<1073741824?`${(n/1048576).toFixed(1)} MB`:`${(n/1073741824).toFixed(1)} GB`;};
export function WorkspaceWindow({open,onClose,devices}:{open:boolean;onClose:()=>void;devices:Registry['devices']}){
 const {t}=useI18n(),[clipboard,setClipboard]=useState<Clipboard|null>(null),[revision,setRevision]=useState(0),[busy,setBusy]=useState(false),[progress,setProgress]=useState(''),[error,setError]=useState(''),[edit,setEdit]=useState<Edit|null>(null),[workspacePath,setWorkspacePath]=useState('/');
 const working=useRef(false),input=useRef<HTMLInputElement>(null),uploadTarget=useRef<Location>({deviceId:'',path:'/uploads'});
 const [devicePaths,setDevicePaths]=useState<Record<string,string>>({});
 const phone=devices.find(d=>d.kind==='phone'&&d.runtimeState==='managed'),computer=devices.find(d=>d.kind==='desktop'&&d.runtimeState==='managed');
 const refresh=()=>setRevision(v=>v+1);
 async function manage(operation:FileOperation,sources:Location[],target?:Location,name=''){
  if(working.current)return;working.current=true;setBusy(true);setError('');let complete=true;
  try{
   const batches=sources.length?sources.map(source=>[source]):[[]];
   const failures:string[]=[],remaining:Location[]=[];
   for(let i=0;i<batches.length;i++){
    setProgress(`${i+1} / ${batches.length}`);
    const result=await fileRpc('manage',FileManageRequestSchema,{operation,sources:batches[i],target,name},FileManageResultSchema);
    const failed=result.results.filter(r=>r.error);if(failed.length){complete=false;failures.push(...failed.map(r=>`${r.source?.path||''}: ${r.error}`));remaining.push(...batches[i]);}
   }
   setError(failures.join('\n'));
   if(operation===FileOperation.MOVE)setClipboard(complete?null:{sources:remaining,operation});
  }catch(e){setError(e instanceof Error?e.message:String(e));complete=false;}
  finally{working.current=false;setBusy(false);setProgress('');refresh();}
  return complete;
 }
 async function upload(files:globalThis.File[],target:Location){
  if(working.current||!files.length)return;working.current=true;setBusy(true);setError('');
  try{for(let i=0;i<files.length;i++){
   setProgress(`${i+1} / ${files.length}`);const source=await uploadFile(files[i],target.deviceId?'/':target.path||'/');
   if(target.deviceId){
    const result=await fileRpc('manage',FileManageRequestSchema,{operation:FileOperation.COPY,sources:[source],target,name:files[i].name},FileManageResultSchema);
    const failure=result.results.find(r=>r.error);if(failure)throw Error(failure.error);
   }
  }if(!target.deviceId)setWorkspacePath(target.path||'/uploads');}
  catch(e){setError(e instanceof Error?e.message:String(e));}finally{working.current=false;setBusy(false);setProgress('');refresh();}
 }
 async function download(sources:Location[]){
  if(working.current)return;working.current=true;setBusy(true);setError('');
  try{
  for(const source of sources){
   let path=source.path||'';
   if(source.deviceId){
    const leaf=path.split('/').at(-1)||'file',dot=leaf.lastIndexOf('.'),suffix=' ('+clientId().slice(0,8)+')',name=dot>0?leaf.slice(0,dot)+suffix+leaf.slice(dot):leaf+suffix;
    const result=await fileRpc('manage',FileManageRequestSchema,{operation:FileOperation.COPY,sources:[source],target:{path:'/'},name},FileManageResultSchema);
    const item=result.results[0];if(!item||item.error)throw Error(item?.error||t('下载失败'));path=item.target?.path||'';refresh();
   }
   const a=document.createElement('a');a.href='/api/workbench/file?path='+encodeURIComponent(path.replace(/^\//,''));a.download=source.path?.split('/').at(-1)||'';a.click();
  }
  }finally{working.current=false;setBusy(false);}
 }
 const pane=(id:string,title:string,deviceId:string|undefined,icon:React.ReactNode)=><FilePane key={id} id={id} title={title} deviceId={deviceId} icon={icon} open={open} revision={revision} busy={busy} clipboard={clipboard} onClipboard={setClipboard} onManage={manage} onEdit={setEdit} onDownload={sources=>void download(sources).catch(e=>setError(e instanceof Error?e.message:String(e)))} onUpload={(target)=>{uploadTarget.current=target;input.current?.click();}} onDropFiles={(files,target)=>void upload(files,target)} controlledPath={id==='workspace'?workspacePath:devicePaths[deviceId||id]||'/'} onPath={id==='workspace'?setWorkspacePath:value=>setDevicePaths(old=>({...old,[deviceId||id]:value}))}/>;
 async function submitEdit(){if(!edit)return;const op=edit.kind==='delete'?FileOperation.DELETE:edit.kind==='rename'?FileOperation.RENAME:FileOperation.MKDIR;const success=await manage(op,edit.sources,edit.location,edit.value);if(success)setEdit(null);}
 return <FloatingWindow id="workspace-files" title={t('文件管理')} open={open} onClose={onClose} width={1120} height={760}>
  <input ref={input} type="file" multiple hidden onChange={e=>{void upload(Array.from(e.target.files||[]),uploadTarget.current);e.target.value='';}}/>
  <div className="file-manager" aria-busy={busy}>
   {(error||busy)&&<div className="file-manager-status" role={error?'alert':'status'}>{busy&&<LoaderCircle size={14} className="spinning"/>}<span title={error}>{error||progress}</span>{error&&<Button variant="ghost" size="icon" aria-label={t('关闭错误')} onClick={()=>setError('')}><X size={14}/></Button>}</div>}
   <div className="file-manager-workspace">{pane('workspace',t('工作文件'),'',<Folder size={16}/>)}</div>
   <div className="file-manager-devices">{pane('phone',phone?.name||t('手机'),phone?.id,<Smartphone size={16}/>)}{pane('computer',computer?.name||t('电脑'),computer?.id,<Monitor size={16}/>)}</div>
   {edit&&<form className="file-manager-edit" onSubmit={e=>{e.preventDefault();void submitEdit();}}><label>{t(edit.kind==='delete'?'删除选中的 {count} 项？':edit.kind==='rename'?'重命名':'新建文件夹',{count:edit.sources.length})}</label>{edit.kind!=='delete'&&<input className="file-edit-input" autoFocus aria-label={t('名称')} value={edit.value} onChange={e=>setEdit({...edit,value:e.target.value})}/>}<Button type="submit" size="icon" disabled={busy||(edit.kind!=='delete'&&!edit.value.trim())} aria-label={t('确认')}><Check size={17}/></Button><Button type="button" variant="ghost" size="icon" aria-label={t('取消')} onClick={()=>setEdit(null)}><X size={17}/></Button></form>}
  </div>
 </FloatingWindow>;
}
function FilePane({id,title,deviceId,icon,open,revision,busy,clipboard,onClipboard,onManage,onEdit,onDownload,onUpload,onDropFiles,controlledPath,onPath}:{
 id:string;title:string;deviceId:string|undefined;icon:React.ReactNode;open:boolean;revision:number;busy:boolean;clipboard:Clipboard|null;onClipboard:(v:Clipboard)=>void;onManage:(op:FileOperation,sources:Location[],target?:Location,name?:string)=>Promise<boolean|undefined>;onEdit:(value:Edit)=>void;onDownload:(v:Location[])=>void;onUpload:(target:Location)=>void;onDropFiles:(files:globalThis.File[],target:Location)=>void;controlledPath:string;onPath:(value:string)=>void;
}){
 const {t,locale}=useI18n(),path=controlledPath,[address,setAddress]=useState(path),[entries,setEntries]=useState<Entry[]>([]),[selected,setSelected]=useState<Set<string>>(new Set()),[loading,setLoading]=useState(false),[error,setError]=useState(''),[over,setOver]=useState(''),[reload,setReload]=useState(0);
 const selection=useRef(selected);selection.current=selected;
 const target={deviceId:deviceId||'',path},sources=entries.filter(e=>selected.has(e.path)).map(e=>({deviceId:deviceId||'',path:e.path})),all=entries.length>0&&selected.size===entries.length,partial=selected.size>0&&!all;
 const go=onPath;
 useEffect(()=>setAddress(path),[path]);
 useEffect(()=>{if(!open||deviceId===undefined)return;const controller=new AbortController();setLoading(true);setError('');setSelected(new Set());
  fileRpc('list',FileLocationSchema,{deviceId,path},FileListingSchema,controller.signal).then(result=>{if(controller.signal.aborted)return;setEntries(result.entries);const canonical=result.location?.path||'/';if(canonical!==path)go(canonical);}).catch(e=>{if(!controller.signal.aborted){setEntries([]);setError(e instanceof Error?e.message:String(e));}}).finally(()=>{if(!controller.signal.aborted)setLoading(false);});
  return()=>controller.abort();
 // Path owner is either the workspace or this pane; requests are cancelled on navigation.
 },[open,deviceId,path,revision,reload]);
 const action=(label:string,Icon:typeof Copy,click:()=>void,disabled=false)=><Button key={label} type="button" variant="ghost" size="icon" aria-label={t(label)} title={t(label)} disabled={busy||disabled||deviceId===undefined} onClick={click}><Icon size={16}/></Button>;
 function select(value:string,checked:boolean){setSelected(old=>{const next=new Set(old);checked?next.add(value):next.delete(value);return next;});}
 function drag(event:DragEvent,entry:Entry){const paths=selection.current.has(entry.path)?entries.filter(e=>selection.current.has(e.path)).map(e=>e.path):[entry.path];event.dataTransfer.setData(MIME,JSON.stringify(paths.map(path=>({deviceId:deviceId||'',path}))));event.dataTransfer.effectAllowed='copy';if(!selection.current.has(entry.path))setSelected(new Set([entry.path]));}
 async function drop(event:DragEvent,directory=path){event.preventDefault();event.stopPropagation();setOver('');if(busy||deviceId===undefined)return;const dest={deviceId:deviceId||'',path:directory};if(event.dataTransfer.files.length){onDropFiles(Array.from(event.dataTransfer.files),dest);return;}const payload=event.dataTransfer.getData(MIME);if(payload){const items:Location[]=JSON.parse(payload);await onManage(FileOperation.COPY,items,dest);}}
 function allow(event:DragEvent,directory:string){if(!busy&&deviceId!==undefined&&(event.dataTransfer.types.includes(MIME)||event.dataTransfer.types.includes('Files'))){event.preventDefault();event.stopPropagation();event.dataTransfer.dropEffect='copy';setOver(directory);}}
 function key(event:KeyboardEvent){if(event.target instanceof HTMLInputElement)return;const cmd=event.metaKey||event.ctrlKey;const k=event.key.toLowerCase();if(cmd&&k==='a'){event.preventDefault();setSelected(new Set(entries.map(e=>e.path)));}else if(cmd&&(k==='c'||k==='x')&&sources.length){event.preventDefault();onClipboard({sources,operation:k==='x'?FileOperation.MOVE:FileOperation.COPY});}else if(cmd&&k==='v'&&clipboard&&!busy){event.preventDefault();void onManage(clipboard.operation,clipboard.sources,target);}else if(event.key==='Delete'&&sources.length){event.preventDefault();onEdit({kind:'delete',pane:id,location:target,sources,value:''});}}
 return <section className="file-pane" aria-label={title} tabIndex={0} onKeyDown={key} data-over={over===path} onDragOver={e=>allow(e,path)} onDragLeave={e=>{if(!e.currentTarget.contains(e.relatedTarget as Node))setOver('');}} onDrop={e=>void drop(e)}>
  <header className="file-pane-header"><h3>{icon}<span>{title}</span></h3><div>{action('刷新',RefreshCw,()=>setReload(v=>v+1))}{action('上传',Upload,()=>onUpload(target))}{action('新建文件夹',FolderPlus,()=>onEdit({kind:'mkdir',pane:id,location:target,sources:[],value:''}))}</div></header>
  <div className="file-pane-toolbar">{selected.size?<><span className="file-selection-count">{selected.size}</span>{action('复制',Copy,()=>onClipboard({sources,operation:FileOperation.COPY}))}{action('剪切',Scissors,()=>onClipboard({sources,operation:FileOperation.MOVE}))}{action('下载',Download,()=>onDownload(sources),entries.some(e=>selected.has(e.path)&&e.directory))}{action('重命名',Pencil,()=>onEdit({kind:'rename',pane:id,location:target,sources,value:entries.find(e=>selected.has(e.path))?.name||''}),selected.size!==1)}{action('删除',Trash2,()=>onEdit({kind:'delete',pane:id,location:target,sources,value:''}))}{action('取消选择',X,()=>setSelected(new Set()))}</>:<><Button variant="ghost" size="icon" aria-label={t('上一级目录')} disabled={path==='/'||deviceId===undefined} onClick={()=>go(parent(path))}><ArrowUp size={16}/></Button><form onSubmit={e=>{e.preventDefault();go(address.startsWith('/')?address:'/'+address);}}><input aria-label={t('路径')} value={address} onChange={e=>setAddress(e.target.value)} spellCheck={false} dir="ltr" disabled={deviceId===undefined}/></form></>}{clipboard&&action('粘贴',ClipboardPaste,()=>void onManage(clipboard.operation,clipboard.sources,target))}</div>
  <div className="file-list-scroll" aria-busy={loading}><table className="file-list"><thead><tr><th className="file-check"><input type="checkbox" aria-label={t('全选')} checked={all} ref={el=>{if(el)el.indeterminate=partial;}} disabled={!entries.length||busy} onChange={e=>setSelected(e.target.checked?new Set(entries.map(e=>e.path)):new Set())}/></th><th>{t('名称')}</th><th className="file-size">{t('大小')}</th><th className="file-modified">{t('修改时间')}</th></tr></thead><tbody>{entries.map(entry=><tr key={entry.path} data-selected={selected.has(entry.path)} data-over={over===entry.path} draggable={!busy} onDragStart={e=>drag(e,entry)} onDragOver={entry.directory?e=>allow(e,entry.path):undefined} onDrop={entry.directory?e=>void drop(e,entry.path):undefined}>
   <td className="file-check"><input type="checkbox" checked={selected.has(entry.path)} aria-label={t('选择 {name}',{name:entry.name})} disabled={busy} onChange={e=>select(entry.path,e.target.checked)}/></td><td><button className="file-name" title={entry.name} onClick={()=>entry.directory||entry.symlink?go(entry.path):onDownload([{deviceId:deviceId||'',path:entry.path}])}>{entry.directory?<Folder size={16}/>:<File size={16}/>}<span>{entry.name}</span></button></td><td className="file-size">{entry.directory?'—':bytes(entry.sizeBytes)}</td><td className="file-modified"><time dateTime={entry.modifiedAtMs?new Date(Number(entry.modifiedAtMs)).toISOString():undefined}>{entry.modifiedAtMs?new Intl.DateTimeFormat(locale,{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit'}).format(Number(entry.modifiedAtMs)):'—'}</time></td>
  </tr>)}</tbody></table>{(loading||error||deviceId===undefined||!entries.length)&&<div className="file-pane-empty" role={error?'alert':'status'}>{loading?<LoaderCircle size={18} className="spinning"/>:error||t(deviceId===undefined?'尚未安装设备':'暂无文件')}</div>}</div>
 </section>;
}
