import {readFileSync} from "node:fs";
import {ROOT,output} from "./tooling/process";
import {uploadPlan,verifyUploaded} from "./tooling/package-delivery";
const [operation,input,...args]=process.argv.slice(2),value=JSON.parse(readFileSync(input!,"utf8"));
if(operation==="plan") console.log(JSON.stringify(await uploadPlan(value,output(["git","rev-parse","HEAD"],{cwd:ROOT}))));
else if(operation==="verify-upload") console.log(JSON.stringify({verified:verifyUploaded(value,JSON.parse(readFileSync(args[args.indexOf("--release")+1]!,"utf8")))}));
else throw Error("Expected plan|verify-upload");
