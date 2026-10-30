import { assistantError, assistantToken } from "@/lib/assistant-server";
import { authenticated } from "@/lib/session-server";
export const runtime = "nodejs";
export async function GET(
  request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;
  const session=await authenticated(request); if(session!==true)return assistantError(session===false?401:503,session===false?"login_required":"auth_unavailable",session===false?"请先登录":"登录服务暂不可用，请重试");
  if (!/^[0-9a-f-]{36}$/i.test(id))
    return assistantError(404, "not_found", "附件不存在");
  try {
    const response = await fetch("http://127.0.0.1:17863/v1/attachment/" + id, {
      headers: { authorization: "Bearer " + (await assistantToken()) },
      cache: "no-store",
      signal: request.signal,
    });
    return new Response(await response.arrayBuffer(), {
      status: response.status,
      headers: {
        "content-type":
          response.headers.get("content-type") ?? "application/octet-stream",
        "cache-control": "private,no-store",
        "x-content-type-options": "nosniff",
        "cross-origin-resource-policy": "same-origin",
      },
    });
  } catch {
    return assistantError(503, "attachment_unavailable", "附件暂不可用");
  }
}
