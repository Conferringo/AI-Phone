import {expect,test} from "bun:test";
import {eventId,permitted,validBinding,matchPairing,makePairing} from "./core";
test("provider identities cannot collide and retries retain their request UUID",()=>{
 expect(eventId("qq","app","event")).toBe(eventId("qq","app","event"));
 expect(eventId("qq","app","event")).not.toBe(eventId("feishu","app","event"));
 expect(eventId("qq","app","event")).not.toBe(eventId("qq","other","event"));
 expect(eventId("qq","app","event")).toMatch(/^[a-f0-9-]{36}$/);
});
test("only explicitly allowed human senders can issue phone tasks",()=>{
 expect(permitted([],"user")).toBe(false);expect(permitted(["user"],"other")).toBe(false);
 expect(permitted(["user"],"user",true)).toBe(false);expect(permitted(["user"],"user")).toBe(true);
});
test("binding credentials are independent of opaque sender IDs",()=>{
 expect(validBinding("cli_app","test_secret")).toBe(true);expect(validBinding("https://external/","test_secret")).toBe(false);expect(validBinding("app","")).toBe(false);
});
test("pairing requires the current private code, expires and limits attempts",()=>{
 const pair=makePairing(1000);expect(matchPairing(pair,"配对 "+pair.code,false,1001)).toBe(false);
 expect(matchPairing(pair,"配对 "+pair.code,true,1001)).toBe(true);
 expect(matchPairing(pair,"配对 "+pair.code,true,700000)).toBe(false);
 pair.attempts=10;expect(matchPairing(pair,"配对 "+pair.code,true,1001)).toBe(false);
});
