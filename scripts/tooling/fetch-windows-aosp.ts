import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";
import { Transform } from "node:stream";

const destination = process.argv[2];
if (!destination) throw Error("Provide the offline factory download destination");
const build = "13455678";
const target = "aosp_cf_x86_64_only_phone-userdebug";
const archive = `aosp_cf_x86_64_only_phone-img-${build}.zip`;
const publicPage = `https://ci.android.com/builds/submitted/${build}/${target}/latest/view/${archive}`;
const listing = await fetch(publicPage);
if (!listing.ok) throw Error(`Android CI viewer returned ${listing.status}`);
const viewer = (await listing.text()).match(/var JSVariables = (\{.*?\});/);
const url = viewer && new URL(JSON.parse(viewer[1]!).artifactUrl);
if (!url || url.protocol !== "https:" || url.hostname !== "storage.googleapis.com")
  throw Error("Android CI did not return the pinned image storage URL");
const response = await fetch(url);
if (!response.ok || !response.body) throw Error(`Android CI image download returned ${response.status}`);
let bytes = 0;
const checksum = createHash("md5");
const meter = new Transform({
  transform(chunk: Buffer, _encoding, done) {
    bytes += chunk.length;
    checksum.update(chunk);
    done(null, chunk);
  },
});
await pipeline(response.body, meter, createWriteStream(destination, { flags: "wx" }));
if (bytes !== 1_001_184_764 || checksum.digest("hex") !== "453044532a2070651c2576dc0343b1b6")
  throw Error("Android CI image differs from the pinned build");

