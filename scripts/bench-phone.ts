import {create,fromBinary,toBinary} from "@bufbuild/protobuf";
import {readFileSync,writeFileSync} from "node:fs";
import {DeviceRequestSchema,DeviceObservationSchema,TakeControlRequestSchema,ControlStateSchema,DeviceActionRequestSchema,DeviceActionResponseSchema} from "../packages/protocol/src/gen/ai_phone/v1/phone_runtime_pb";
const token=readFileSync("data/phone-runner/token","utf8").trim();
async function rpc(op:string,schema:any,value:any,out:any){const started=performance.now();const r=await fetch("http://127.0.0.1:17862/v1/"+op,{method:"POST",headers:{authorization:"Bearer "+token,"content-type":"application/x-protobuf"},body:toBinary(schema,create(schema,value))});if(!r.ok)throw new Error(op+":"+r.status);return {ms:performance.now()-started,value:fromBinary(out,new Uint8Array(await r.arrayBuffer())) as any};}
const scope={phoneId:"phone-local-01",ownerId:"local-user"};
const {value:control}=await rpc("control",TakeControlRequestSchema,{...scope,controller:"manual"},ControlStateSchema);
const summary=(items:number[])=>{items.sort((a,b)=>a-b);return {medianMs:+items[Math.floor(items.length/2)].toFixed(1),p95Ms:+items[Math.min(items.length-1,Math.floor(items.length*.95))].toFixed(1),samples:items.length};};
const screen:number[]=[],tap:number[]=[];
for(let i=0;i<8;i++){let r=await rpc("observe",DeviceRequestSchema,scope,DeviceObservationSchema);screen.push(r.ms);r=await rpc("action",DeviceActionRequestSchema,{...scope,controller:"manual",controlEpoch:control.controlEpoch,runId:"latency-bench",actionId:crypto.randomUUID(),action:{case:"tap",value:{x:710,y:1200}}},DeviceActionResponseSchema);if(!r.value.success)throw new Error("tap failed");tap.push(r.ms);}
const report={date:"2026-10-26",stage:process.argv[2]||"baseline",screen:summary(screen),tap:summary(tap)};writeFileSync("docs/evidence/p2/latency-"+report.stage+".json",JSON.stringify(report,null,2)+"\n");console.log(JSON.stringify(report,null,2));
