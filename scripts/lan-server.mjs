import http from "node:http";
import os from "node:os";
import {writeFileSync,mkdirSync} from "node:fs";
import path from "node:path";
function privateIPv4(ip){const parts=ip.split('.').map(Number);return parts.length===4&&(parts[0]===10||(parts[0]===172&&parts[1]>=16&&parts[1]<=31)||(parts[0]===192&&parts[1]===168));}
const interfaces=os.networkInterfaces();
const address=process.env.AI_PHONE_LAN_IP??[...interfaces.en1??[],...interfaces.en0??[],...Object.entries(interfaces).filter(([name])=>/^en\d+$/.test(name)).flatMap(([,addresses])=>addresses??[])].find(i=>i.family==='IPv4'&&!i.internal&&privateIPv4(i.address))?.address;
if(!address||!privateIPv4(address))throw new Error("No private LAN interface is available");
const root=process.env.AI_PHONE_PROJECT_ROOT??process.cwd();mkdirSync(path.join(root,'data'),{recursive:true});
const origin=`http://${address}:17860`;
const server=http.createServer((request,response)=>{
  const peer=request.socket.remoteAddress?.replace(/^::ffff:/,'')??'';
  if(!privateIPv4(peer)&&peer!=='127.0.0.1'){response.writeHead(403);response.end();return;}
  if(!request.url?.startsWith('/')){response.writeHead(400);response.end();return;}
  const headers={...request.headers,'x-forwarded-for':peer,'x-forwarded-proto':'http','x-forwarded-host':request.headers.host};
  const upstream=http.request({host:'127.0.0.1',port:17860,path:request.url,method:request.method,headers},reply=>{response.writeHead(reply.statusCode??502,reply.headers);reply.pipe(response);});
  upstream.on('error',()=>{if(!response.headersSent)response.writeHead(503);response.end('Service temporarily unavailable');});
  response.on('close',()=>upstream.destroy());
  request.on('aborted',()=>upstream.destroy());request.pipe(upstream);
});
server.listen(17860,address,()=>{writeFileSync(path.join(root,'data/deployment.json'),JSON.stringify({origin,lan_ip:address,port:17860},null,2)+'\n',{mode:0o600});console.log(`AI Phone LAN: ${origin}`);});
