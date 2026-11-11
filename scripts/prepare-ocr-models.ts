import {join} from "node:path";
import {ROOT} from "./tooling/process";
import {json} from "./tooling/inventory";
import {pinnedDownload} from "./tooling/android-component";
const args=process.argv.slice(2),destination=args[args.indexOf("--destination")+1];
if(!destination) throw Error("--destination is required");
const policy=json(join(ROOT,"config/ocr-models.json"));
for(const model of policy.models)await pinnedDownload(model,join(destination,model.file));
console.log(JSON.stringify({models:policy.models.length,source_commit:policy.source_commit}));
