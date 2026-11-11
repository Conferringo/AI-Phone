import {relative} from "node:path";
import {ROOT,output,digest} from "./tooling/process";
import {save} from "./tooling/inventory";
import {buildImageHandoff,imageUploadPlan,fetchImages,discardImages} from "./tooling/image-handoff";
const [operation,input,...args]=process.argv.slice(2),option=(name:string)=>args.includes(name)?args[args.indexOf(name)+1]:undefined,source=output(["git","rev-parse","HEAD"],{cwd:ROOT});
let result:unknown;
if(operation==="build"){if(!option("--result"))throw Error("build requires --result");const path=await buildImageHandoff(ROOT,input!,source);result={report:relative(ROOT,path),metadata_sha256:await digest(path)};}
else if(operation==="plan")result=await imageUploadPlan(ROOT,input!,source);
else if(operation==="fetch")result={directory:await fetchImages(ROOT,input!,option("--identity")!,source,option("--work")!,Number(option("--release-id")))};
else if(operation==="discard"){discardImages(input!,option("--identity")!,source,Number(option("--release-id")));result={discarded:true};}
else throw Error("Expected build|plan|fetch|discard");
if(option("--result"))save(option("--result")!,result);else console.log(JSON.stringify(result));
