// Actual WorkspaceWindow callbacks with modeled React state; no DOM/device claim.
import {mock} from 'bun:test';
import {plugin} from 'bun';
import assert from 'node:assert/strict';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const Op={COPY:1,MOVE:2,RENAME:3,DELETE:4,MKDIR:5};
mock.module('@ai-phone/protocol/ai_phone/v1/workbench_pb',()=>({FileLocationSchema:{},FileListingSchema:{},FileManageRequestSchema:{},FileManageResultSchema:{},FileOperation:Op}));
const element=(type:unknown,props:any)=>({type,props});
const stub=()=>null;
const component=(name:string)=>join(process.env.AI_PHONE_TEST_CANDIDATE_ROOT!,'apps/web/src/components',name);
mock.module('lucide-react',()=>Object.fromEntries('ArrowUp Check Copy Scissors ClipboardPaste FolderPlus Folder File Download Upload Trash2 Pencil RefreshCw LoaderCircle Smartphone Monitor X'.split(' ').map(key=>[key,stub])));
mock.module(component('floating-window.tsx'),()=>({FloatingWindow:stub}));
mock.module(component('ui/button.tsx'),()=>({Button:stub}));
mock.module(component('locale-provider.tsx'),()=>({useI18n:()=>({t:(value:string)=>value,locale:'en'})}));
mock.module('@/lib/client-id',()=>({clientId:()=>{throw Error('unexpected download');}}));
const slots:any[]=[];let cursor=0;
mock.module('react',()=>({
 useRef:(value:any)=>{const i=cursor++;return slots[i]||(slots[i]={current:value});},
 useState:(value:any)=>{const i=cursor++;if(!(i in slots))slots[i]=value;return[slots[i],(next:any)=>{slots[i]=typeof next==='function'?next(slots[i]):next;}];},
 useEffect:()=>{},
}));
mock.module('react/jsx-runtime',()=>({jsx:element,jsxs:element,Fragment:'fragment'}));
mock.module('react/jsx-dev-runtime',()=>({jsxDEV:element,Fragment:'fragment'}));
const calls:any[]=[];let reply:(request:any)=>any=()=>({results:[]});
mock.module('@/lib/file-manager-client',()=>({fileRpc:async(operation:string,_schema:any,request:any)=>{assert.equal(operation,'manage');calls.push(request);return reply(request);},uploadFile:()=>{throw Error('unexpected upload');}}));
plugin({name:'file-manager-test-css',setup(build){build.onResolve({filter:/\.css$/},args=>({path:args.path,namespace:'test-css'}));build.onLoad({filter:/.*/,namespace:'test-css'},()=>({contents:'export {};',loader:'js'}));}});
const {WorkspaceWindow}=await import(pathToFileURL(join(process.env.AI_PHONE_TEST_CANDIDATE_ROOT!,'apps/web/src/components/workspace-window.tsx')).href);
function find(node:any,predicate:(node:any)=>boolean):any{
 if(!node||typeof node!=='object')return;
 if(Array.isArray(node)){for(const child of node){const result=find(child,predicate);if(result)return result;}return;}
 return predicate(node)?node:find(node.props?.children,predicate);
}
let tree:any;
function render(){cursor=0;tree=WorkspaceWindow({open:true,onClose:stub,devices:[]});return find(tree,node=>node.props?.id==='workspace').props;}
const sources=Array.from({length:24},(_,i)=>({deviceId:'phone',path:'/Download/'+i+'.csv'}));
const target={deviceId:'desktop',path:'/home/ai/Files'};
reply=request=>({results:request.sources.map((source:any)=>({source,target,error:''}))});
assert.equal(await render().onManage(Op.COPY,sources,target),true);
assert.equal(calls.length,1,'multi-selection must produce one RPC, not one round trip per file');
assert.deepEqual(calls[0].sources,sources);
assert.deepEqual(calls[0].target,target);

calls.length=0;
let pane=render();pane.onClipboard({sources,operation:Op.MOVE});
reply=request=>({results:request.sources.map((source:any,i:number)=>({source,target,error:i===7?'destination exists':''}))});
assert.equal(await render().onManage(Op.MOVE,sources,target),false);
assert.equal(calls.length,1);
pane=render();assert.deepEqual(pane.clipboard,{sources:[sources[7]],operation:Op.MOVE});
assert.match(find(tree,node=>node.props?.role==='alert').props.children[1].props.children,/7\.csv: destination exists/);
reply=request=>({results:request.sources.map((source:any)=>({source,target,error:''}))});
assert.equal(await pane.onManage(pane.clipboard.operation,pane.clipboard.sources,target),true);
assert.deepEqual(calls[1].sources,[sources[7]],'completed moves must not be replayed');
assert.equal(render().clipboard,null);

calls.length=0;
reply=()=>{throw Error('connection lost');};
pane=render();pane.onClipboard({sources,operation:Op.MOVE});
assert.equal(await render().onManage(Op.MOVE,sources,target),false);
assert.equal(calls.length,1,'unknown outcomes must not automatically retry');
assert.deepEqual(render().clipboard.sources,sources);
assert.equal(find(tree,node=>node.props?.role==='alert').props.children[1].props.children,'connection lost');

calls.length=0;
reply=()=>({results:[{target,error:''}]});
assert.equal(await render().onManage(Op.MKDIR,[],target,'Inbox'),true);
assert.equal(calls.length,1);assert.deepEqual(calls[0].sources,[]);assert.equal(calls[0].name,'Inbox');

calls.length=0;
let finish:(value:any)=>void;
reply=()=>new Promise(resolve=>{finish=resolve;});
pane=render();const pending=pane.onManage(Op.DELETE,sources);
assert.equal(await pane.onManage(Op.DELETE,sources),undefined);
assert.equal(calls.length,1,'the same user operation must not start twice while pending');
finish!({results:sources.map(source=>({source,error:''}))});
assert.equal(await pending,true);assert.equal(render().busy,false);
console.log('file manager batch fixture passed');
