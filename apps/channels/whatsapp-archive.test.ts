import {test,expect} from 'bun:test';
import {createRequire} from 'node:module';
import {realpathSync,mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';

const require=createRequire(realpathSync(import.meta.dir+'/node_modules/whatsapp-web.js/package.json'));
const archiver=require('archiver'),unzipper=require('unzipper');
test('security-pinned glob and minimatch preserve SDK archive directory and pattern behavior',async()=>{
 const root=mkdtempSync(join(tmpdir(),'aiphone-archive-'));
 try{
  mkdirSync(join(root,'nested'));mkdirSync(join(root,'ignored'));
  for(const [name,text] of [['one.txt','one'],['nested/中文.txt','中文'],['.hidden.txt','hidden'],['ignored/skip.txt','skip'],['nested/other.md','other']])writeFileSync(join(root,name),text);
  for(const mode of ['directory','pattern']){
   const archive=archiver('zip',{zlib:{level:1}}),chunks:Buffer[]=[];
   const complete=new Promise<Buffer>((resolve,reject)=>{archive.on('data',(b:Buffer)=>chunks.push(b));archive.on('error',reject);archive.on('end',()=>resolve(Buffer.concat(chunks)));});
   if(mode==='directory')archive.directory(root,false);
   else archive.glob('**/*.txt',{cwd:root,dot:true,ignore:['ignored/**']});
   await archive.finalize();const zip=await unzipper.Open.buffer(await complete);
   const files=new Map<string,string>();for(const entry of zip.files)if(entry.type==='File')files.set(entry.path,(await entry.buffer()).toString());
   expect(files.get('nested/中文.txt')).toBe('中文');expect(files.get('.hidden.txt')).toBe('hidden');expect(files.get('one.txt')).toBe('one');
   expect(files.has('nested/other.md')).toBe(mode==='directory');expect(files.has('ignored/skip.txt')).toBe(mode==='directory');
  }
 }finally{rmSync(root,{recursive:true,force:true})}
});
