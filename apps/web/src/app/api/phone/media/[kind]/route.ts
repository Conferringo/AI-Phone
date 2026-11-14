import {relayMedia} from '@/lib/media-relay';
import {phoneTarget} from '@/lib/phone-target-server';
import {authenticated, requestSessionToken, validSession} from '@/lib/session-server';
import {assistantError} from '@/lib/assistant-server';

export const runtime = 'nodejs';

export async function GET(request: Request, {params}: {params: Promise<{kind: string}>}) {
  const session = await authenticated(request);
  if (session !== true) {
    return assistantError(session === false ? 401 : 503, 'unavailable', session === false ? '请先登录' : '服务正在恢复');
  }
  const {kind} = await params;
  if (!['screen', 'audio'].includes(kind)) return assistantError(404, 'not_found', '没有这项媒体');
  const query = new URL(request.url).searchParams;
  try {
    const {token, media} = await phoneTarget(query.get('device') || 'phone-local-01');
    return await relayMedia(
      request.signal,
      signal => fetch(`http://127.0.0.1:${media}/${kind}`, {
        headers: {authorization: 'Bearer ' + token}, cache: 'no-store', signal,
      }),
      () => validSession(requestSessionToken(request)),
      kind === 'screen' && query.get('raw') === '1',
    );
  } catch {
    return assistantError(503, 'media_unavailable', '手机媒体正在恢复');
  }
}
