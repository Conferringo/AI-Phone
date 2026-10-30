import {readFile,access} from 'node:fs/promises';
import path from 'node:path';
export async function phoneTarget(id:string){
 const root=process.env.AI_PHONE_PROJECT_ROOT||path.resolve(process.cwd(),'../..');
 if(id==='phone-local-01')return {runner:17862,media:17865,token:(await readFile(path.join(root,'data/phone-runner/token'),'utf8')).trim()};
 if(!/^[0-9a-f-]{36}$/.test(id))throw Error('unknown_phone');
 if(await access(path.join(root,'data/phones/second/archived')).then(()=>true,()=>false))throw Error('device_archived');
 const entry=JSON.parse(await readFile(path.join(root,'data/phones/second.json'),'utf8'));
 if(entry.device_id!==id||entry.slot!==2)throw Error('unknown_phone');
 return {runner:17872,media:17875,token:(await readFile(path.join(root,'data/phones/second/runner/token'),'utf8')).trim()};
}
