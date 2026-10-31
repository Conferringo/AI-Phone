import {readFileSync,existsSync,constants,accessSync} from 'node:fs';
import {resolve,isAbsolute,relative} from 'node:path';
export function whatsappBrowser(root:string,env:NodeJS.ProcessEnv=process.env):string{
 const custom=env.AI_PHONE_WHATSAPP_CHROMIUM;
 if(custom){if(!isAbsolute(custom))throw Error('WhatsApp浏览器路径必须是绝对路径');accessSync(custom,constants.X_OK);return custom;}
 const base=resolve(root,'.tools/whatsapp-browser'),manifest=resolve(base,'browser.json');
 if(!existsSync(manifest))throw Error('WhatsApp浏览器组件尚未安装');
 const saved=JSON.parse(readFileSync(manifest,'utf8'));
 if(saved.format!==1||typeof saved.executable!=='string'||isAbsolute(saved.executable))throw Error('WhatsApp浏览器组件清单无效');
 const executable=resolve(base,saved.executable),path=relative(base,executable);
 if(path.startsWith('..')||isAbsolute(path))throw Error('WhatsApp浏览器路径无效');
 accessSync(executable,constants.X_OK);return executable;
}
export function whatsappProxy(value:string){
 if(!value)return {args:[] as string[]};
 const url=new URL(value);
 if(!['http:','https:','socks5:'].includes(url.protocol)||url.search||url.hash||!['','/'].includes(url.pathname))throw Error('WhatsApp代理地址无效');
 if(url.protocol==='socks5:'&&(url.username||url.password))throw Error('SOCKS代理暂不支持账号认证');
 const auth=url.username?{username:decodeURIComponent(url.username),password:decodeURIComponent(url.password)}:undefined;
 url.username='';url.password='';return {args:[`--proxy-server=${url.toString().replace(/\/$/,'')}`],auth};
}
