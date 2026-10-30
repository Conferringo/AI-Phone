import {test,expect} from 'bun:test';
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {discoveryDirectories,emulatorDiscovery} from '../apps/device-stream/emulator-discovery';

test('Linux discovers the owned registration and refuses another device or unauthenticated gRPC',()=>{
 const dir=mkdtempSync(join(tmpdir(),'aiphone-discovery-'));
 try{
  writeFileSync(join(dir,'pid_123.ini'),'grpc.port=8558\nport.serial=5566\ngrpc.token=other\n');
  expect(()=>emulatorDiscovery(8558,'emulator-5564',[dir])).toThrow('emulator_unavailable');
  writeFileSync(join(dir,'pid_124.ini'),'grpc.port=8558\nport.serial=5564\ngrpc.token=fixture-only\n');
  expect(emulatorDiscovery(8558,'emulator-5564',[dir]).token).toBe('fixture-only');
  writeFileSync(join(dir,'pid_124.ini'),'grpc.port=8558\nport.serial=5564\n');
  expect(()=>emulatorDiscovery(8558,'emulator-5564',[dir])).toThrow('emulator_auth_unavailable');
 }finally{rmSync(dir,{recursive:true,force:true});}
});
test('Linux, macOS and Windows use their platform discovery roots',()=>{
 expect(discoveryDirectories('linux',{ANDROID_EMULATOR_HOME:'/private/android'},'/home/test',1000)).toContain('/private/android/avd/running');
 expect(discoveryDirectories('darwin',{},'/fixture/home',501)).toEqual(['/fixture/home/Library/Caches/TemporaryItems/avd/running']);
 expect(discoveryDirectories('win32',{LOCALAPPDATA:'/test/local'},'/test',0)).toEqual(['/test/local/Temp/avd/running']);
});
