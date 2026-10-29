import { readRequestBody } from '@/lib/request-body';
import {
  allowedOrigin,
  assistantError,
  assistantToken,
} from "@/lib/assistant-server";
import { authenticated } from "@/lib/session-server";
export const runtime = "nodejs";
export async function POST(
  request: Request,
  { params }: { params: Promise<{ operation: string }> },
) {
  const { operation } = await params;
  const session=await authenticated(request); if(session!==true)return assistantError(session===false?401:503,session===false?"login_required":"auth_unavailable",session===false?"请先登录":"登录服务暂不可用，请重试");
  if (
    ![
      "history",
      "send",
      "cancel",
      "resume",
      "upload",
    ].includes(operation)
  )
    return assistantError(404, "not_found", "操作不存在");
  if (!allowedOrigin(request))
    return assistantError(403, "wrong_origin", "请从工作台操作");
  const media = request.headers.get("content-type") ?? "";
  if (operation !== "upload" && media !== "application/x-protobuf")
    return assistantError(415, "unsupported_media_type", "请求格式错误");
  const limit = operation === "upload" ? 9 * 1024 * 1024 : 65536;
  if (Number(request.headers.get("content-length") ?? 0) > limit)
    return assistantError(413, "body_rejected", "内容过大");
  try {
    const body = await readRequestBody(request,limit);if(body instanceof Response)return body;
    const response = await fetch("http://127.0.0.1:17863/v1/" + operation, {
      method: "POST",
      headers: {
        authorization: "Bearer " + (await assistantToken()),
        "content-type": media,
      },
      body,
      cache: "no-store",
      signal: AbortSignal.any([request.signal, AbortSignal.timeout(30000)]),
    });
    return new Response(await response.arrayBuffer(), {
      status: response.status,
      headers: {
        "content-type": "application/x-protobuf",
        "cache-control": "no-store",
      },
    });
  } catch {
    return assistantError(
      503,
      "assistant_unavailable",
      "助手服务暂不可用，请重试",
    );
  }
}
