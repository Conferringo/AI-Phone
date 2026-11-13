'use client';

import {useEffect, useId, useState, type ReactNode} from 'react';
import {Monitor, Plus, Smartphone} from 'lucide-react';
import {LoadingOrb} from '@/components/ui/loading-orb';
import {clientId} from '@/lib/client-id';
import {useI18n} from './locale-provider';
import type {Registry} from './registry-window';
import {DesktopInstall} from './desktop-install';
import {Button} from './ui/button';
import {SelectField} from './ui/select';

export type DeviceDraft = Pick<Registry['devices'][number], 'id' | 'revision' | 'name' | 'kind'>;

export function RegistryAdministration({registry, busy, error, footer, open, activeDeviceId, onRefresh, onSave}: {
  registry: Registry;
  busy: string;
  error: string;
  footer: ReactNode;
  open: boolean;
  activeDeviceId: string;
  onRefresh: () => void;
  onSave: (draft: DeviceDraft) => Promise<boolean>;
}) {
  const {t} = useI18n();
  const formId = useId();
  const [selected, setSelected] = useState(activeDeviceId);
  const [drafts, setDrafts] = useState<Record<string, DeviceDraft>>({});
  const rows = [...registry.devices, ...Object.values(drafts).filter(row =>
    row.revision === 0n && !registry.devices.some(device => device.id === row.id))];
  const row = rows.find(item => item.id === selected) ?? rows[0];
  const draft = row && drafts[row.id];
  const form = draft ?? row;
  const device = registry.devices.find(item => item.id === row?.id);
  const limits = {phone: registry.phoneLimit, desktop: registry.desktopLimit};
  const kinds = (['phone', 'desktop'] as const).map(kind => ({
    value: kind,
    label: t(kind === 'phone' ? 'Android 手机' : 'Linux 电脑'),
    disabled: rows.filter(item => item.kind === kind && (draft?.revision !== 0n || item.id !== draft.id)).length >= limits[kind],
  }));
  const available = kinds.find(kind => !kind.disabled);

  useEffect(() => setSelected(activeDeviceId), [activeDeviceId]);

  function edit(next:DeviceDraft) {
    setDrafts(current => ({...current, [next.id]:next}));
  }

  function discard(id:string) {
    setDrafts(current => {
      const remaining = {...current};
      delete remaining[id];
      return remaining;
    });
  }

  return <section className="registry-administration registry-form" aria-label={t('设备')}>
    <h3 className="setup-title registry-setup-heading" data-setup-heading tabIndex={-1}>{t('设备')}</h3>
    <div className="registry-layout">
      <nav className="registry-list" aria-label={t('设备列表')}>
        <div className="registry-entities">
          {rows.map(item => {
            const Icon = (drafts[item.id]?.kind ?? item.kind) === 'phone' ? Smartphone : Monitor;
            return <button type="button" key={item.id} aria-pressed={item.id === form?.id} disabled={!!busy}
              onClick={() => setSelected(item.id)}><Icon aria-hidden size={18}/><span>{drafts[item.id]?.name || item.name || t('未命名')}</span></button>;
          })}
        </div>
        {available && <button type="button" className="registry-add" disabled={!!busy || draft?.revision === 0n}
          onClick={() => {
            const id = clientId();
            edit({id, revision:0n, name:'', kind:available.value});
            setSelected(id);
          }}>
          <Plus aria-hidden size={16}/>{t('添加设备')}
        </button>}
      </nav>
      <div className="registry-detail">
        {error && <p className="registry-alert" role="alert">{error}</p>}
        {form && <form id={formId} className="registry-form" onSubmit={async event => {
          event.preventDefault();
          if (draft && await onSave(draft)) discard(draft.id);
        }}>
          <div className="registry-scroll">
            <fieldset disabled={!!busy}>
              <label>{t('名称')}<input required maxLength={80} value={form.name}
                onChange={event => edit({...form, name:event.target.value})}/></label>
              {form.revision === 0n
                ? <label>{t('系统')}<SelectField aria-label={t('设备类型')} value={form.kind} options={kinds}
                  onValueChange={kind => edit({...form, kind})}/></label>
                : <div className="registry-system">{device?.system.replace(' · 待安装', '')}</div>}
              {open && device?.kind === 'desktop' && <DesktopInstall key={device.id}
                deviceId={device.id} installed={device.runtimeState === 'managed'} onInstalled={onRefresh}/>}
            </fieldset>
          </div>
        </form>}
      </div>
    </div>
    <footer className="registry-save setup-footer">
      {footer}
      {form && <div className="registry-admin-actions">
        {draft && <Button type="button" variant="ghost" disabled={!!busy} onClick={() => discard(draft.id)}>{t('撤销')}</Button>}
        <Button type="submit" form={formId} disabled={!!busy || !draft || !draft.name.trim()}>
          {busy === 'save' && <LoadingOrb size={15} />}{t(busy === 'save' ? '保存中…' : '保存')}
        </Button>
      </div>}
    </footer>
  </section>;
}
