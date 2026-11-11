import 'server-only';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import type {DeviceDefaults} from '@/components/device-defaults';
/** Read only explicit creation resources after authentication; never send paths/secrets. */
export async function deviceDefaults():Promise<DeviceDefaults>{
 // Private state is read at runtime and may live outside the version's source tree.
 const profilePath=resolve(process.env.AI_PHONE_PROJECT_ROOT||'../..','data/deployment/profile.json');
 let profile:any={};try{const value=JSON.parse(await readFile(/* turbopackIgnore: true */ profilePath,'utf8'));if(value&&typeof value==='object'&&!Array.isArray(value))profile=value;}catch{}
 const select=(value:unknown,allowed:number[],fallback:number)=>typeof value==='number'&&allowed.includes(value)?value:fallback;
 return {phone:{cpus:select(profile.phone?.cpus,[1,2,4],4),memoryMb:select(profile.phone?.memory_mb,[1536,2048,3072,4096],3072),storageGb:select(profile.phone?.storage_gb,[16,24,32],16)},desktop:{cpus:select(profile.desktop?.cpus,[1,2,4],2),memoryMb:select(profile.desktop?.memory_mb,[2048,2560,3072,4096],2560)}};
}
