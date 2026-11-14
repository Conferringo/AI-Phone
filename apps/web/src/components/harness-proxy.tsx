'use client';

import {HarnessProxyRequestSchema, HarnessProxyStateSchema} from '@ai-phone/protocol/ai_phone/v1/harness_proxy_pb';
import type {MessageInitShape, MessageShape} from '@bufbuild/protobuf';
import {Network, RotateCw} from 'lucide-react';
import {useEffect, useId, useState} from 'react';
import {settingsRpc} from '@/lib/settings-client';
import {FloatingWindow, WindowTrigger} from './floating-window';
import {useI18n} from './locale-provider';
import {Button} from './ui/button';
import {LoadingOrb} from './ui/loading-orb';
import {SelectField} from './ui/select';
import './harness-proxy.css';

type ProxyState = MessageShape<typeof HarnessProxyStateSchema>;
type Operation = 'import' | 'refresh' | 'toggle' | 'select';

export function HarnessProxy({open, onOpen, onClose}: {open: boolean; onOpen: () => void; onClose: () => void}) {
  const {t} = useI18n();
  const urlId = useId();
  const [state, setState] = useState<ProxyState>();
  const [url, setUrl] = useState('');
  const [busy, setBusy] = useState<Operation>();
  const [error, setError] = useState('');

  useEffect(() => {
    if (!open) return;
    const request = new AbortController();
    setError('');
    void settingsRpc('state', HarnessProxyRequestSchema, {}, HarnessProxyStateSchema, 'proxy', request.signal)
      .then(next => { if (!request.signal.aborted) setState(next); }, () => {
        if (!request.signal.aborted) setError('无法读取代理设置');
      });
    return () => request.abort();
  }, [open]);

  async function update(operation: Operation, request: MessageInitShape<typeof HarnessProxyRequestSchema>) {
    setBusy(operation);
    setError('');
    try {
      setState(await settingsRpc(operation, HarnessProxyRequestSchema, request, HarnessProxyStateSchema, 'proxy'));
      if (operation === 'import') setUrl('');
    } catch {
      setError({import: '无法导入订阅', refresh: '无法更新订阅', toggle: '无法切换代理', select: '无法切换节点'}[operation]);
    } finally {
      setBusy(undefined);
    }
  }

  const title = t('工作台代理');
  return <>
    <WindowTrigger label={title} open={open} onOpen={onOpen}><Network size={18} fill={state?.enabled ? 'currentColor' : 'none'} fillOpacity={.18}/></WindowTrigger>
    <FloatingWindow id="harness-proxy" title={title} open={open} onClose={onClose} width={420} height={480} fitContent>
      <div className="harness-proxy" aria-busy={!!busy || (!state && !error)}>
        {!state && !error && <LoadingOrb size={18}/>}
        {error && <p role="alert">{t(error)}</p>}
        {state && <>
          <div className="harness-proxy-toggle">
            <label htmlFor={`${urlId}-enabled`}>{t('启用')}</label>
            <input id={`${urlId}-enabled`} type="checkbox" role="switch" checked={state.enabled}
              disabled={!!busy || !state.hasSubscription} onChange={event => void update('toggle', {enabled:event.target.checked})}/>
          </div>
          <form onSubmit={event => { event.preventDefault(); void update('import', {subscriptionUrl:url.trim()}); }}>
            <label htmlFor={urlId}>{t('订阅地址')}</label>
            <div className="harness-proxy-import">
              <input id={urlId} type="password" inputMode="url" dir="ltr" required autoComplete="off" spellCheck={false}
                value={url} placeholder={state.hasSubscription ? t('已保存') : 'https://'}
                onChange={event => setUrl(event.target.value)} disabled={!!busy}/>
              <Button type="submit" disabled={!!busy || !url.trim()}>{busy === 'import' && <LoadingOrb size={14}/>} {t('导入')}</Button>
              {state.hasSubscription && <Button type="button" variant="outline" size="icon" disabled={!!busy}
                title={t('更新订阅')} aria-label={t('更新订阅')} onClick={() => void update('refresh', {})}>
                {busy === 'refresh' ? <LoadingOrb size={14}/> : <RotateCw size={16}/>}
              </Button>}
            </div>
          </form>
          {state.groups.map(group => <div className="harness-proxy-group" key={group.name}>
            <span>{group.name}</span>
            <SelectField withinWindow aria-label={group.name} value={group.selected} disabled={!!busy}
              options={group.nodes.map(node => ({value:node, label:node}))}
              onValueChange={node => void update('select', {group:group.name, node})}/>
          </div>)}
        </>}
      </div>
    </FloatingWindow>
  </>;
}
