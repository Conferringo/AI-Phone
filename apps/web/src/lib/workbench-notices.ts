export type WorkbenchNotice={message:string;scope:string;label:string;retry?:()=>void};
export type VisibleNotice=WorkbenchNotice&{id:string};
const identity=(notice:WorkbenchNotice)=>JSON.stringify([notice.scope,notice.message]);

/** A mounted source owns its notice; polling cannot undo a user's dismissal. */
export class WorkbenchNoticeStore {
 private sources=new Map<string,WorkbenchNotice>();
 private dismissed=new Set<string>();
 private listeners=new Set<()=>void>();
 private visible:VisibleNotice[]=[];
 snapshot=()=>this.visible;
 subscribe=(listener:()=>void)=>{this.listeners.add(listener);return()=>{this.listeners.delete(listener);};};
 set(source:string,notice:WorkbenchNotice|null){
  const old=this.sources.get(source);
  if(notice&&old&&identity(old)===identity(notice)&&old.label===notice.label&&old.retry===notice.retry)return;
  if(notice)this.sources.set(source,notice);else if(!this.sources.delete(source))return;
  this.publish();
 }
 dismiss(id:string){this.dismissed.add(id);this.publish();}
 private publish(){
  const grouped=new Map<string,VisibleNotice>();
  for(const notice of this.sources.values()){
   const id=identity(notice),previous=grouped.get(id);
   grouped.set(id,{...notice,id,retry:notice.retry||previous?.retry});
  }
  for(const id of this.dismissed)if(!grouped.has(id))this.dismissed.delete(id);
  this.visible=[...grouped.values()].filter(notice=>!this.dismissed.has(notice.id));
  for(const listener of this.listeners)listener();
 }
}
