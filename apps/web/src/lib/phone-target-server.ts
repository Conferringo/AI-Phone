import {readFile,access} from 'node:fs/promises';
import path from 'node:path';
export async function phoneTarget(id:string){
 const root=process.env.AI_PHONE_PROJECT_ROOT||path.resolve(/* turbopackIgnore: true */ process.cwd(),'../..');
 if(id==='phone-local-01'){
  const tokenPath=path.join(/* turbopackIgnore: true */ root,'data/phone-runner/token');
  return {runner:17862,media:17865,token:(await readFile(/* turbopackIgnore: true */ tokenPath,'utf8')).trim()};
 }
 if(!/^[0-9a-f-]{36}$/.test(id))throw Error('unknown_phone');
 const archivedPath=path.join(/* turbopackIgnore: true */ root,'data/phones/second/archived');
 if(await access(/* turbopackIgnore: true */ archivedPath).then(()=>true,()=>false))throw Error('device_archived');
 const entryPath=path.join(/* turbopackIgnore: true */ root,'data/phones/second.json');
 const entry=JSON.parse(await readFile(/* turbopackIgnore: true */ entryPath,'utf8'));
 if(entry.device_id!==id||entry.slot!==2)throw Error('unknown_phone');
 const tokenPath=path.join(/* turbopackIgnore: true */ root,'data/phones/second/runner/token');
 return {runner:17872,media:17875,token:(await readFile(/* turbopackIgnore: true */ tokenPath,'utf8')).trim()};
}
