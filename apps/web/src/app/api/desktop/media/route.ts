import {relayMedia} from '@/lib/media-relay';
import {authenticated, requestSessionToken, validSession} from '@/lib/session-server';
import {create, fromBinary, toBinary} from '@bufbuild/protobuf';
import {RegistryRequestSchema, RegistryStateSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {assistantToken, assistantError} from '@/lib/assistant-server';
import {readFile} from 'node:fs/promises';
import path from 'node:path';

export const runtime = 'nodejs';

export async function GET(request: Request) {
  const session = await authenticated(request);
  if (session !== true) {
    return assistantError(session === false ? 401 : 503, 'unavailable', session === false ? '请先登录' : '服务正在恢复');
  }
  const query = new URL(request.url).searchParams;
  const id = query.get('device_id');
  const kind = query.get('kind') === 'audio' ? 'audio' : 'screen';
  try {
    const registry = await fetch('http://127.0.0.1:17863/v1/registry/state', {
      method: 'POST', cache: 'no-store',
      headers: {authorization: 'Bearer ' + await assistantToken(), 'content-type': 'application/x-protobuf'},
      body: toBinary(RegistryRequestSchema, create(RegistryRequestSchema)),
      signal: AbortSignal.any([request.signal, AbortSignal.timeout(5000)]),
    });
    if (!registry.ok) return assistantError(503, 'unavailable', '设备服务正在恢复');
    const state = fromBinary(RegistryStateSchema, new Uint8Array(await registry.arrayBuffer()));
    if (!state.devices.some(device => device.id === id && device.kind === 'desktop' && device.runtimeState === 'managed')) {
      return assistantError(404, 'not_found', '电脑尚未接入');
    }
    const root = process.env.AI_PHONE_PROJECT_ROOT || path.resolve(process.cwd(), '../..');
    const token = (await readFile(path.join(root, 'data/phone-runner/token'), 'utf8')).trim();
    return await relayMedia(
      request.signal,
      signal => fetch(`http://127.0.0.1:17865/desktop/${kind}`, {
        headers: {authorization: 'Bearer ' + token}, cache: 'no-store', signal,
      }),
      () => validSession(requestSessionToken(request)),
      kind === 'screen' && query.get('raw') === '1',
    );
  } catch {
    return assistantError(503, 'media_unavailable', '电脑画面正在恢复');
  }
}
