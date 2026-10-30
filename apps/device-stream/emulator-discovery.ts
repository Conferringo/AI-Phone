import {homedir} from 'node:os';
import {join} from 'node:path';
import {readdirSync,readFileSync,statSync} from 'node:fs';

export function discoveryDirectories(platform:string,env:Record<string,string|undefined>,home:string,uid:number){
 const roots=platform==='darwin'?[join(home,'Library/Caches/TemporaryItems')]:platform==='win32'?[env.LOCALAPPDATA&&join(env.LOCALAPPDATA,'Temp')]:[env.XDG_RUNTIME_DIR,'/run/user/'+uid,env.ANDROID_EMULATOR_HOME,env.ANDROID_PREFS_ROOT,env.ANDROID_SDK_HOME,join(home,'.android')];
 return [...new Set(roots.filter((root):root is string=>!!root))].map(root=>join(root,'avd/running'));
}

export function emulatorDiscovery(port:number,serial:string,directories=discoveryDirectories(process.platform,process.env,homedir(),process.getuid?.()??0)){
 const paths=directories.flatMap(directory=>{try{return readdirSync(directory).filter(name=>/^pid_\d+\.ini$/.test(name)).map(name=>join(directory,name));}catch{return [];}});
 const files=paths.flatMap(path=>{try{const stat=statSync(path);if(process.getuid&&stat.uid!==process.getuid())return [];return [{path,at:stat.mtimeMs}];}catch{return [];}}).sort((a,b)=>b.at-a.at);
 for(const {path} of files){
  let text:string;try{text=readFileSync(path,'utf8');}catch{continue;}
  const values=Object.fromEntries(text.split(/\r?\n/).filter(line=>line.includes('=')).map(line=>{const at=line.indexOf('=');return [line.slice(0,at),line.slice(at+1)];}));
  if(values['grpc.port']!==String(port)||(values['port.serial']&&'emulator-'+values['port.serial']!==serial))continue;
  if(!values['grpc.token'])throw Error('emulator_auth_unavailable');
  return {path,token:values['grpc.token']};
 }
 throw Error('emulator_unavailable');
}
