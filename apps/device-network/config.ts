import {lookup} from 'node:dns/promises';
import {isIP} from 'node:net';
export function publicAddress(ip:string){
 if(isIP(ip)===6)return /^[23]/i.test(ip)&&!/^2001:db8:/i.test(ip);
 const p=ip.split('.').map(Number);
 return isIP(ip)===4&&p[0]!==0&&p[0]!==10&&p[0]!==127&&p[0]<224&&!(p[0]===169&&p[1]===254)&&!(p[0]===172&&p[1]>=16&&p[1]<=31)&&!(p[0]===192&&[0,168].includes(p[1]))&&!(p[0]===198&&[18,19].includes(p[1]))&&!(p[0]===198&&p[1]===51&&p[2]===100)&&!(p[0]===203&&p[1]===0&&p[2]===113)&&!(p[0]===100&&p[1]>=64&&p[1]<=127);
}
export function subscriptionUrl(raw:string){
 let url:URL;try{url=new URL(raw);}catch{throw Error('请输入有效的 HTTPS 订阅地址');}
 if(url.protocol!=='https:'||url.username||url.password||url.hash||url.href.length>8192)throw Error('订阅仅支持不含账号字段的 HTTPS 地址');
 const host=url.hostname.replace(/^\[|\]$/g,'');
 if(host==='localhost'||host.endsWith('.local')||host.endsWith('.localhost')||(isIP(host)&&!publicAddress(host)))throw Error('订阅地址不能指向本机或私有网络');
 return url.href;
}
export async function downloadSubscription(raw:string){
 // Pin public DNS for every redirect; never inherit the host's proxy agent.
 const {request}=await import('node:https');let url=subscriptionUrl(raw);
 for(let hop=0;hop<4;hop++){
  const parsed=new URL(url),addresses=await lookup(parsed.hostname,{all:true});
  if(!addresses.length||addresses.some(a=>!publicAddress(a.address)))throw Error('订阅域名解析到受限地址');
  const selected=addresses.find(a=>a.family===4)||addresses[0];
  const response=await new Promise<{status:number;location?:string;body:Buffer}>((resolve,reject)=>{
   const req=request(parsed,{method:'GET',agent:false,headers:{'user-agent':'clash.meta AI-Phone/0.1','accept':'application/yaml,text/yaml,text/plain'},lookup:((_h:any,o:any,cb:any)=>o?.all?cb(null,[selected]):cb(null,selected.address,selected.family)) as any},res=>{
    let size=0;const parts:Buffer[]=[];
    res.on('data',(part:Buffer)=>{size+=part.length;if(size>2*1024*1024){req.destroy();reject(Error('订阅内容过大'));}else parts.push(part);});
    res.on('end',()=>resolve({status:res.statusCode||0,location:res.headers.location,body:Buffer.concat(parts)}));res.on('error',()=>reject(Error('订阅下载中断')));
   });
   req.setTimeout(25000,()=>req.destroy(Error('订阅下载超时')));req.on('error',()=>reject(Error('订阅下载失败，请检查地址和直联网络')));req.end();
  });
  if([301,302,303,307,308].includes(response.status)&&response.location){url=subscriptionUrl(new URL(response.location,url).href);continue;}
  if(response.status!==200)throw Error(`订阅服务器返回 HTTP ${response.status}`);
  return response.body.toString('utf8');
 }
 throw Error('订阅重定向过多');
}
export function makeConfig(yaml:string,secret:string,kind:'phone'|'desktop',tun=true){
 let source:any;try{source=Bun.YAML.parse(yaml);}catch{throw Error('订阅不是有效的 Clash YAML');}
 if(!Array.isArray(source?.proxies)||!source.proxies.length||source.proxies.length>512)throw Error('订阅需包含 1–512 个 Clash 节点');
 const nodes=source.proxies;const names=new Set<string>();
 const types=['ss','ssr','vmess','vless','trojan','hysteria','hysteria2','tuic','anytls','socks5','http','wireguard','snell'];let budget=0,bytes=0;const ancestors=new Set<object>();
 function inspect(value:any,depth=0){
  if(++budget>30000||depth>12)throw Error('订阅结构过于复杂');
  if(typeof value==='string'){bytes+=Buffer.byteLength(value);if(bytes>2*1024*1024||value.length>32768)throw Error('订阅内容过大');}
  if(value&&typeof value==='object'){
   if(ancestors.has(value))throw Error('订阅不能包含循环引用');ancestors.add(value);
   for(const [key,entry]of Object.entries(value)){if(/(?:[-_](?:path|file)$)|script/i.test(key)||['file','ca','certificate'].includes(key))throw Error('订阅节点不能引用文件或脚本');inspect(entry,depth+1);}ancestors.delete(value);
  }
 }
 for(const node of nodes){if(!node||typeof node.name!=='string'||!node.name.trim()||node.name.length>200||names.has(node.name)||!types.includes(node.type)||typeof node.server!=='string'||!node.server||!Number.isInteger(node.port)||node.port<1||node.port>65535)throw Error('订阅节点格式无效或名称重复');inspect(node);names.add(node.name);}
 const providerDns=source.dns?.['proxy-server-nameserver'];
 if(providerDns!==undefined&&(!Array.isArray(providerDns)||providerDns.length>8||providerDns.some((v:any)=>typeof v!=='string'||v.length>2048||/[\r\n]/.test(v)||!(/^(https:\/\/|tls:\/\/|tcp:\/\/|udp:\/\/|\d+\.\d+\.\d+\.\d+)/.test(v)))))throw Error('订阅的节点 DNS 配置无效');
 // Import nodes only: upstream listeners, scripts, paths and control settings are discarded.
 return {'mixed-port':17890,'allow-lan':false,'bind-address':'127.0.0.1',mode:'rule','log-level':'silent',ipv6:false,'external-controller':'127.0.0.1:19090',secret,'interface-name':'eth0',...(kind==='desktop'?{'routing-mark':9488}:{}),profile:{'store-selected':true},'geo-auto-update':false,'geodata-mode':false,
  dns:{enable:true,listen:'127.0.0.1:1053',ipv6:false,'enhanced-mode':'redir-host','default-nameserver':['223.5.5.5','1.1.1.1'],nameserver:['https://dns.alidns.com/dns-query','https://1.1.1.1/dns-query'],'proxy-server-nameserver':providerDns?.length?providerDns:['https://dns.alidns.com/dns-query']},
  sniffer:{enable:true,'override-destination':true,sniff:{TLS:{ports:[443,8443]},HTTP:{ports:[80,8080]}}},
  tun:{enable:tun,device:'aiphone0',stack:'gvisor','auto-route':true,'auto-detect-interface':true,'strict-route':true,mtu:1500,'dns-hijack':['any:53','tcp://any:53'],'route-exclude-address':['127.0.0.0/8','10.0.0.0/8','172.16.0.0/12','192.168.0.0/16'],...(kind==='phone'?{'include-android-user':[0],'exclude-uid':[0]}:{})},
  proxies:nodes,'proxy-groups':[{name:'AI Phone',type:'url-test',proxies:[...names],url:'https://www.gstatic.com/generate_204',interval:300,tolerance:100,timeout:5000}],
  rules:['DOMAIN-SUFFIX,local,DIRECT','IP-CIDR,127.0.0.0/8,DIRECT,no-resolve','IP-CIDR,10.0.0.0/8,DIRECT,no-resolve','IP-CIDR,172.16.0.0/12,DIRECT,no-resolve','IP-CIDR,192.168.0.0/16,DIRECT,no-resolve','DOMAIN-SUFFIX,qq.com,DIRECT','DOMAIN-SUFFIX,weixin.com,DIRECT','DOMAIN-SUFFIX,wechat.com,DIRECT','DOMAIN-SUFFIX,gtimg.com,DIRECT','GEOIP,CN,DIRECT,no-resolve','MATCH,AI Phone']};
}
