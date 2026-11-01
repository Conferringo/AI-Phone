import * as grpc from '@grpc/grpc-js';
import {loadSync} from '@grpc/proto-loader';
import {emulatorDiscovery} from './emulator-discovery';

type PhoneRuntime={sdk:string;port:number;serial:string};
type DiscoveredPhone={path:string;token:string};
export class PhoneRuntimeUnavailable extends Error {
 readonly code='phone_runtime_unavailable';
 constructor(){super('phone_runtime_unavailable');}
}

function loadController(sdk:string):grpc.ServiceClientConstructor {
 const definition=grpc.loadPackageDefinition(loadSync(sdk+'/emulator/lib/emulator_controller.proto',{
  keepCase:false,longs:String,enums:String,defaults:true,oneofs:true,
 }));
 const controller=(definition as unknown as {android:{emulation:{control:{EmulatorController:grpc.ServiceClientConstructor}}}}).android.emulation.control.EmulatorController;
 if(typeof controller!=='function')throw new PhoneRuntimeUnavailable();
 return controller;
}

/** Desktop routes do not depend on an Android SDK or a running phone. */
export function createPhoneGrpc(runtime:PhoneRuntime,discover:()=>DiscoveredPhone=()=>emulatorDiscovery(runtime.port,runtime.serial)){
 let Controller:grpc.ServiceClientConstructor|undefined,client:InstanceType<grpc.ServiceClientConstructor>|undefined,cachedPath='',closed=false;
 return {
  connection(){
   try{
    if(closed)throw new PhoneRuntimeUnavailable();
    Controller??=loadController(runtime.sdk);
    const {path,token}=discover();
    if(!client||cachedPath!==path){
     const next=new Controller('127.0.0.1:'+runtime.port,grpc.credentials.createInsecure(),{'grpc.max_receive_message_length':12*1024*1024});
     client?.close();client=next;cachedPath=path;
    }
    const metadata=new grpc.Metadata();metadata.set('authorization','Bearer '+token);
    return {client,metadata};
   }catch{throw new PhoneRuntimeUnavailable();}
  },
  close(){closed=true;client?.close();client=undefined;},
 };
}
