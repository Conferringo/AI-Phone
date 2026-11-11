import { readFile } from "node:fs/promises";
import path from "node:path";
import { readFileSync } from "node:fs";
import { create, toBinary } from "@bufbuild/protobuf";
import { ApiErrorSchema } from "@ai-phone/protocol/ai_phone/v1/system_pb";
export function assistantError(status: number, code: string, message: string) {
  return new Response(
    toBinary(ApiErrorSchema, create(ApiErrorSchema, { code, message })),
    {
      status,
      headers: {
        "content-type": "application/x-protobuf",
        "cache-control": "no-store",
      },
    },
  );
}
export function loopbackOrigin(request: Request) {
  const origin = request.headers.get("origin");
  return origin === "http://127.0.0.1:17860" || origin === "http://localhost:17860";
}
export function allowedOrigin(request: Request) {
  if (loopbackOrigin(request)) return true;
  try {
    const root=process.env.AI_PHONE_PROJECT_ROOT??path.resolve(/* turbopackIgnore: true */ process.cwd(),"../..");
    const deploymentPath=path.join(/* turbopackIgnore: true */ root,"data/deployment.json");
    // Persistent configuration is runtime state, not a build asset.
    const origin = request.headers.get("origin");
    return origin !== null && origin === JSON.parse(readFileSync(/* turbopackIgnore: true */ deploymentPath,"utf8")).origin;
  } catch {}
  return false;
}
export async function assistantToken() {
  const root =
    process.env.AI_PHONE_PROJECT_ROOT ?? path.resolve(/* turbopackIgnore: true */ process.cwd(), "../..");
  // Private state belongs to the persistent user directory and is read at runtime.
  const tokenPath = path.join(/* turbopackIgnore: true */ root, "data/assistant/token");
  return (
    await readFile(/* turbopackIgnore: true */ tokenPath, "utf8")
  ).trim();
}
