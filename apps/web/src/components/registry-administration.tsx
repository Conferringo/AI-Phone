'use client';

import { LoadingOrb } from "@/components/ui/loading-orb";
import {useEffect, useId, useState, type ReactNode} from 'react';
import {Monitor, Plus, RefreshCw, Smartphone, Sparkles} from 'lucide-react';
import {clientId} from '@/lib/client-id';
import {useI18n} from './locale-provider';
import type {Registry} from './registry-window';
import {AgentConnections} from './connections-dialog';
import {AgentMemory} from './memory-window';
import {AgentVoice} from './agent-voice';
import {DesktopInstall} from './desktop-install';
import {Button} from './ui/button';
import {SelectField} from './ui/select';

type DeviceDraft = Pick<Registry['devices'][number], 'id' | 'revision' | 'name' | 'kind' | 'defaultAgentId'>;
type AgentDraft = Pick<Registry['agents'][number], 'id' | 'revision' | 'name' | 'deviceIds' | 'instructions'> & {modelId: string};
type Tab = 'devices' | 'agents';
type Section = 'details' | 'connections' | 'memory' | 'voice';

type Props = {
  registry: Registry;
  busy: string;
  error: string;
  footer: ReactNode;
  open: boolean;
  voiceAgentId: string;
  activeDeviceId: string;
  onRefresh: () => void;
  onSaveAgent: (draft: AgentDraft) => Promise<boolean>;
  onSaveDevice: (draft: DeviceDraft) => Promise<boolean>;
};

function deviceIcon(kind: string) {
  return kind === 'phone' ? <Smartphone aria-hidden size={18}/> : <Monitor aria-hidden size={18}/>;
}

function without<T>(drafts: Record<string, T>, id: string) {
  const remaining = {...drafts};
  delete remaining[id];
  return remaining;
}

export function RegistryAdministration({registry, busy, error, footer, open, voiceAgentId, activeDeviceId, onRefresh, onSaveAgent, onSaveDevice}: Props) {
  const {t} = useI18n();
  const formId = useId();
  const [tab, setTab] = useState<Tab>('devices');
  const [section, setSection] = useState<Section>('details');
  const [selectedDevice, setSelectedDevice] = useState('');
  const [selectedAgent, setSelectedAgent] = useState('');
  const [deviceEdits, setDeviceEdits] = useState<Record<string, DeviceDraft>>({});
  const [agentEdits, setAgentEdits] = useState<Record<string, AgentDraft>>({});

  useEffect(() => setSelectedDevice(activeDeviceId), [activeDeviceId]);
  useEffect(() => {
    if (open && voiceAgentId) {
      setTab('agents');
      setSelectedAgent(voiceAgentId);
      setSection('voice');
    }
  }, [open, voiceAgentId]);

  const models = registry.models.filter(model => !!model.profile?.verifiedAtMs);
  const deviceRows = [...registry.devices, ...Object.values(deviceEdits).filter(draft => draft.revision === 0n && !registry.devices.some(device => device.id === draft.id))];
  const agentRows = [...registry.agents, ...Object.values(agentEdits).filter(draft => draft.revision === 0n && !registry.agents.some(agent => agent.id === draft.id))];
  const device = deviceRows.find(row => row.id === selectedDevice) ?? deviceRows[0];
  const agent = agentRows.find(row => row.id === selectedAgent) ?? agentRows[0];
  const deviceDraft = device && (deviceEdits[device.id] ?? device);
  const agentDraft = agent && (agentEdits[agent.id] ?? {...agent, modelId: 'modelConnectionId' in agent ? agent.modelConnectionId : agent.modelId});
  const savedDevice = registry.devices.find(item => item.id === deviceDraft?.id);
  const form = tab === 'devices' ? deviceDraft : agentDraft;
  const dirty = !!form && !!(tab === 'devices' ? deviceEdits[form.id] : agentEdits[form.id]);
  const canSave = dirty && !!form?.name.trim() && (tab === 'devices' || models.some(model => model.id === agentDraft?.modelId));
  const limits = {phone: registry.phoneLimit, desktop: registry.desktopLimit};
  const kinds = (['phone', 'desktop'] as const).map(kind => {
    const used = deviceRows.filter(row => (deviceEdits[row.id]?.kind ?? row.kind) === kind).length;
    return {
      value: kind,
      label: t(kind === 'phone' ? 'Android 手机' : 'Linux 电脑'),
      available: used < limits[kind],
      disabled: used - Number(deviceDraft?.kind === kind) >= limits[kind],
    };
  });
  const kindToAdd = kinds.find(kind => kind.available);

  function choose(next: Tab, id?: string) {
    setTab(next);
    setSection('details');
    if (id) {
      if (next === 'devices') setSelectedDevice(id);
      else setSelectedAgent(id);
    }
  }

  function add() {
    const id = clientId();
    if (tab === 'devices') {
      if (!kindToAdd) return;
      const draft: DeviceDraft = {id, revision: 0n, name: '', kind: kindToAdd.value, defaultAgentId: ''};
      setDeviceEdits(previous => ({...previous, [id]: draft}));
    } else {
      const draft: AgentDraft = {id, revision: 0n, name: '', modelId: models[0]?.id ?? '', deviceIds: [], instructions: ''};
      setAgentEdits(previous => ({...previous, [id]: draft}));
    }
    choose(tab, id);
  }

  function editDevice(patch: Partial<DeviceDraft>) {
    if (deviceDraft) setDeviceEdits(previous => ({...previous, [deviceDraft.id]: {...deviceDraft, ...patch}}));
  }

  function editAgent(patch: Partial<AgentDraft>) {
    if (agentDraft) setAgentEdits(previous => ({...previous, [agentDraft.id]: {...agentDraft, ...patch}}));
  }

  function discard() {
    if (!form) return;
    if (tab === 'devices') setDeviceEdits(previous => without(previous, form.id));
    else setAgentEdits(previous => without(previous, form.id));
  }

  async function save() {
    if (busy || !canSave) return;
    const saved = tab === 'devices'
      ? deviceDraft && await onSaveDevice({...deviceDraft, defaultAgentId: savedDevice?.defaultAgentId ?? ''})
      : agentDraft && await onSaveAgent(agentDraft);
    if (saved) discard();
  }

  const showDetails = tab === 'devices' || section === 'details';
  const rows = tab === 'devices'
    ? deviceRows.map(row => ({...row, name: deviceEdits[row.id]?.name ?? row.name, icon: deviceIcon(deviceEdits[row.id]?.kind ?? row.kind)}))
    : agentRows.map(row => ({...row, name: agentEdits[row.id]?.name ?? row.name, icon: <Sparkles aria-hidden size={18}/>}));

  return <section className="registry-administration registry-form">
    <nav className="registry-tabs" aria-label={t('配置分类')}>
      <button type="button" aria-pressed={tab === 'devices'} disabled={!!busy} onClick={() => choose('devices')}><Monitor aria-hidden size={16}/>{t('设备')}</button>
      <button type="button" aria-pressed={tab === 'agents'} disabled={!!busy} onClick={() => choose('agents')}><Sparkles aria-hidden size={16}/>Agent</button>
      <button type="button" className="registry-reload" aria-label={t('重新载入配置')} title={t('重新载入配置')} disabled={!!busy} onClick={onRefresh}><RefreshCw aria-hidden size={15}/></button>
    </nav>
    <div className="registry-layout">
      <nav className="registry-list" aria-label={t(tab === 'devices' ? '设备列表' : 'Agent 列表')}>
        <div className="registry-entities">
          {rows.map(row => <button type="button" key={row.id} aria-pressed={row.id === form?.id} disabled={!!busy} onClick={() => choose(tab, row.id)}>
            {row.icon}<span>{row.name || t('未命名')}</span>
            {(tab === 'devices' ? deviceEdits[row.id] : agentEdits[row.id]) && <i aria-label={t('未保存')}/>}
          </button>)}
        </div>
        <button type="button" className="registry-add" disabled={!!busy || (tab === 'devices' ? !kindToAdd : !models.length || agentRows.length >= 64)} onClick={add}>
          <Plus aria-hidden size={16}/>{t(tab === 'devices' ? '添加设备' : '添加 Agent')}
        </button>
      </nav>
      <div className="registry-detail">
        {tab === 'agents' && agentDraft && <nav className="registry-sections" aria-label={t('Agent 配置')}>
          {([['details', '配置'], ['connections', '连接应用'], ['memory', '记忆'], ['voice', '语音']] as const).map(([value, label]) => <button key={value} type="button" aria-pressed={section === value} disabled={!!busy || (agentDraft.revision === 0n && value !== 'details')} onClick={() => setSection(value)}>{t(label)}</button>)}
        </nav>}
        {error && <div className="registry-alert" role="alert">{error}</div>}
        {showDetails && form && <form id={formId} className="registry-form" onSubmit={event => {event.preventDefault(); void save();}}>
          <div className="registry-scroll">
            <fieldset disabled={!!busy}>
              <label>{t('名称')}<input required maxLength={80} value={form.name} onChange={event => {
                const name = event.target.value;
                if (tab === 'devices') editDevice({name});
                else editAgent({name});
              }}/></label>
              {tab === 'devices' && deviceDraft && <>
                {deviceDraft.revision === 0n
                  ? <label>{t('系统')}<SelectField aria-label={t('设备类型')} disabled={!!busy} value={deviceDraft.kind} options={kinds} onValueChange={kind => editDevice({kind})}/></label>
                  : <div className="registry-system">{deviceIcon(deviceDraft.kind)}<span>{savedDevice?.system.replace(' · 待安装', '')}</span></div>}
                {open && deviceDraft.kind === 'desktop' && savedDevice && <DesktopInstall key={deviceDraft.id} deviceId={deviceDraft.id} installed={savedDevice.runtimeState === 'managed'} onInstalled={onRefresh}/>}
              </>}
              {tab === 'agents' && agentDraft && <>
                <label>{t('模型')}<SelectField aria-label={t('模型')} disabled={!!busy} value={models.some(model => model.id === agentDraft.modelId) ? agentDraft.modelId : ''} onValueChange={modelId => editAgent({modelId})} options={[
                  {value: '', label: t('未配置'), disabled: true},
                  ...models.map(model => ({value: model.id, label: model.name === model.profile?.model ? model.name : `${model.name} · ${model.profile?.model}`})),
                ]}/></label>
                <div className="registry-bindings"><h3>{t('可用设备')}</h3>
                  {registry.devices.map(item => <label key={item.id}>{deviceIcon(item.kind)}<span>{item.name}</span><input
                    aria-label={t('授权 {0}', {'0': item.name})}
                    type="checkbox"
                    disabled={item.defaultAgentId === agentDraft.id}
                    checked={agentDraft.deviceIds.includes(item.id)}
                    onChange={event => editAgent({deviceIds: event.target.checked ? [...agentDraft.deviceIds, item.id] : agentDraft.deviceIds.filter(id => id !== item.id)})}
                  /></label>)}
                </div>
              </>}
            </fieldset>
          </div>
        </form>}
        {registry.agents.map(item => {
          const visible = tab === 'agents' && item.id === agentDraft?.id && section !== 'details';
          return <div key={item.id} className="registry-scoped" hidden={!visible}>
            <div hidden={section !== 'connections'}><AgentConnections agentId={item.id} open={open && visible && section === 'connections'}/></div>
            <div hidden={section !== 'memory'}><AgentMemory agentId={item.id} open={open && visible && section === 'memory'}/></div>
            <div hidden={section !== 'voice'} className="registry-voice-panel"><AgentVoice agentId={item.id} open={open && visible && section === 'voice'}/></div>
          </div>;
        })}
      </div>
    </div>
    <footer className="registry-save setup-footer">
      {footer}
      {showDetails && form && <div className="registry-admin-actions">
        {dirty && <Button type="button" variant="ghost" disabled={!!busy} onClick={discard}>{t('撤销')}</Button>}
        <Button type="submit" form={formId} disabled={!!busy || !canSave}>{busy === 'save' && <LoadingOrb size={15} />}{t(busy === 'save' ? '保存中…' : '保存')}</Button>
      </div>}
    </footer>
  </section>;
}
