import http from "node:http";
// Test-only negative control. It never forwards traffic or changes Clash.
const server = http.createServer((request,response)=>{
  console.log("REJECT HTTP",request.method,request.url);
  response.writeHead(502,{"content-length":"0"}); response.end();
});
server.on("connect",(request,socket)=>{
  console.log("REJECT CONNECT",request.url);
  socket.end("HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
});
server.listen(17869,"127.0.0.1",()=>console.log("Negative control proxy listening on 127.0.0.1:17869"));
