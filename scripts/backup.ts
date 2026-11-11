import {ROOT,nativeProgram,run} from "./tooling/process";
const [operation,...args]=process.argv.slice(2);
run([nativeProgram(ROOT),"--state-backup",operation!,...args,...(args.includes("--root")?[]:["--root",ROOT])]);
