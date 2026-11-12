'use client';

import {create, type MessageShape} from '@bufbuild/protobuf';
import {ModelDraftSchema, ModelCredentialSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
import {ConnectionCredentialRequestSchema, type RegisteredAgentSchema, type RegisteredModelSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {Check, ChevronDown, Eye, EyeOff, Pencil, Plus, Trash2} from 'lucide-react';
import {useEffect, useRef, useState, type ReactNode} from 'react';
import {LoadingOrb} from '@/components/ui/loading-orb';
import {clientId} from '@/lib/client-id';
import {modelPreset, modelPresets, presetDraft} from '@/lib/registry-setup';
import {settingsRpc} from '@/lib/settings-client';
import {AgentConnections} from './connections-dialog';
import {AgentMemory} from './memory-window';
import {AgentVoice} from './agent-voice';
import {useI18n} from './locale-provider';
import {Button} from './ui/button';
import {SelectField} from './ui/select';
import type {Registry} from './registry-window';

type Agent = MessageShape<typeof RegisteredAgentSchema>;
type RegisteredModel = MessageShape<typeof RegisteredModelSchema>;
type Config = MessageShape<typeof ModelDraftSchema>;
type Section = 'config' | 'connections' | 'memory' | 'voice';
export type AgentEditor = {id:string; agent?:Agent; modelId:string; model?:RegisteredModel; name:string; config:Config};

export function modelDraft(model?: RegisteredModel): Config {
  if (!model?.profile) return presetDraft('deepseek');
  const {provider, protocol, baseUrl, model: modelId, proxyUrl, hasApiKey} = model.profile;
  return create(ModelDraftSchema, {provider, protocol, baseUrl, model: modelId, proxyUrl, useSavedKey: hasApiKey});
}

export function AgentLabel({agent, model}: {agent:Agent; model?:RegisteredModel}) {
  const profile = model?.profile;
  const presetId = profile && modelPreset(profile);
  const provider = modelPresets.find(option => option.id !== 'custom' && option.id === presetId)?.label;
  const detail = [provider, profile?.model === agent.name ? '' : profile?.model].filter(Boolean).join(' · ');
  return <span className="registry-model-label"><strong>{agent.name}</strong>{detail && <small>{detail}</small>}</span>;
}

export function RegistryAgents({registry, onboarding, open, voiceAgentId, busy, error, footer, next, onSave, onSelect, onDelete, onClearError}: {
  registry: Registry;
  onboarding: boolean;
  open: boolean;
  voiceAgentId: string;
  busy: string;
  error: string;
  footer: ReactNode;
  next?: ReactNode;
  onSave: (draft: AgentEditor) => Promise<boolean>;
  onSelect: (agent: Agent) => Promise<boolean>;
  onDelete: (agent: Agent) => Promise<boolean>;
  onClearError: () => void;
}) {
  const {t} = useI18n();
  const [editing, setEditing] = useState<{id:string; section:Section} | null>(null);
  const [deleting, setDeleting] = useState('');
  const configured = registry.agents.flatMap(agent => {
    const model = registry.models.find(row => row.id === agent.modelConnectionId);
    return model?.profile ? [{agent, model}] : [];
  });
  const selected = configured.find(({agent}) => agent.id === editing?.id);

  useEffect(() => {
    if (voiceAgentId) setEditing({id:voiceAgentId, section:'voice'});
  }, [voiceAgentId]);

  if (editing) return <AgentForm key={editing.id + ':' + editing.section} id={editing.id} agent={selected?.agent} model={selected?.model}
    initialSection={editing.section} busy={busy} error={error} open={open} onboarding={onboarding}
    onSave={onSave} onClearError={onClearError} onClose={() => {setEditing(null); onClearError();}}/>;

  return <section className="registry-setup-stage" aria-label="Agent">
    <div className="registry-pane-heading">
      <h3 className="setup-title" data-setup-heading tabIndex={-1}>Agent</h3>
      <Button type="button" variant="outline" size="icon" className="registry-model-add" aria-label={t('添加 Agent')}
        title={t('添加 Agent')} disabled={!!busy} onClick={() => {onClearError(); setEditing({id:clientId(), section:'config'});}}>
        <Plus size={19}/>
      </Button>
    </div>
    <div className="registry-setup-scroll">
      {error && <p role="alert" className="registry-setup-error">{error}</p>}
      {configured.length ? <ul className="registry-setup-rows registry-profile-list">
        {configured.map(({agent, model}) => {
          const active = registry.activeAgentId === agent.id;
          return <li key={agent.id} className="registry-profile-row">
            <button type="button" className="registry-profile-select" aria-pressed={active} disabled={!!busy}
              onClick={() => {setDeleting(''); if (!active) void onSelect(agent);}}>
              <span className="registry-profile-mark" aria-hidden>{active && <Check size={14}/>}</span>
              <AgentLabel agent={agent} model={model}/>
            </button>
            {deleting === agent.id ? <div className="registry-admin-actions" role="group" aria-label={t('删除 Agent')}>
              <Button type="button" variant="ghost" autoFocus disabled={!!busy} onClick={() => setDeleting('')}>{t('取消')}</Button>
              <Button type="button" variant="destructive" disabled={!!busy} onClick={async () => {
                if (await onDelete(agent)) setDeleting('');
              }}>{t('删除 Agent')}</Button>
            </div> : <div className="registry-admin-actions">
              <Button type="button" size="icon" variant="ghost" aria-label={`${t('编辑 Agent')} · ${agent.name}`}
                title={t('编辑 Agent')} disabled={!!busy} onClick={() => {onClearError(); setEditing({id:agent.id, section:'config'});}}>
                <Pencil size={16}/>
              </Button>
              <Button type="button" size="icon" variant="ghost" aria-label={`${t('删除 Agent')} · ${agent.name}`}
                title={t('删除 Agent')} disabled={!!busy} onClick={() => {onClearError(); setDeleting(agent.id);}}>
                <Trash2 size={16}/>
              </Button>
            </div>}
          </li>;
        })}
      </ul> : <p className="registry-setup-empty">{t('暂无 Agent')}</p>}
    </div>
    <footer className="registry-save setup-footer">{footer}{next}</footer>
  </section>;
}

function AgentForm({id, agent, model, initialSection, busy, error, open, onboarding, onSave, onClose, onClearError}: {
  id: string;
  agent?: Agent;
  model?: RegisteredModel;
  initialSection: Section;
  busy: string;
  error: string;
  open: boolean;
  onboarding: boolean;
  onSave: (draft: AgentEditor) => Promise<boolean>;
  onClose: () => void;
  onClearError: () => void;
}) {
  const {t} = useI18n();
  const [section, setSection] = useState(initialSection);
  const [modelId] = useState(() => model?.id ?? clientId());
  const [name, setName] = useState(agent?.name ?? '');
  const [config, setConfig] = useState(() => modelDraft(model));
  const [showKey, setShowKey] = useState(false);
  const [revealed, setRevealed] = useState('');
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState('');
  const nameField = useRef<HTMLInputElement>(null);
  useEffect(() => {if (section === 'config') nameField.current?.focus({preventScroll:true});}, [section]);

  function update(patch: Partial<Config>) {
    setConfig(current => ({...current, ...patch}));
    onClearError();
  }

  async function reveal() {
    if (showKey || !config.useSavedKey || revealed) {
      setShowKey(!showKey);
      return;
    }
    if (!model) return;
    setKeyBusy(true);
    setKeyError('');
    try {
      const credential = await settingsRpc('credential', ConnectionCredentialRequestSchema,
        {id:model.id, expectedRevision:model.revision}, ModelCredentialSchema, 'registry');
      setRevealed(credential.apiKey);
      setShowKey(true);
    } catch (failure) {
      setKeyError(failure instanceof Error ? failure.message : t('读取密钥失败'));
    } finally {
      setKeyBusy(false);
    }
  }

  return <section className="registry-setup-stage">
    <h3 className="setup-title registry-setup-heading" data-setup-heading tabIndex={-1}>{t(agent ? '编辑 Agent' : '添加 Agent')}</h3>
    {agent && !onboarding && <nav className="registry-sections" aria-label={t('Agent 配置')}>
      {([['config', '配置'], ['connections', '连接应用'], ['memory', '记忆'], ['voice', '语音']] as const).map(([value, label]) =>
        <button key={value} type="button" aria-pressed={section === value} disabled={!!busy || keyBusy} onClick={() => setSection(value)}>{t(label)}</button>)}
    </nav>}
    <form className="registry-form registry-model-editor" hidden={section !== 'config'} onSubmit={async event => {
      event.preventDefault();
      if (await onSave({id, agent, modelId, model, name, config})) onClose();
    }}>
      <div className="registry-scroll">
        {(error || keyError) && <p role="alert" className="registry-setup-error">{error || keyError}</p>}
        <fieldset disabled={!!busy || keyBusy}>
          <label>{t('名称')}<input ref={nameField} maxLength={80} placeholder={config.model}
            value={name} onChange={event => {setName(event.target.value); onClearError();}}/></label>
          <label>{t('服务商')}<SelectField withinWindow aria-label={t('服务商')} value={modelPreset(config)}
            onValueChange={value => {setRevealed(''); setShowKey(false); update(presetDraft(value));}}
            options={modelPresets.map(preset => ({value:preset.id, label:preset.id === 'custom' ? t('兼容服务') : preset.label}))}/></label>
          <label>{t('模型')}<input aria-label={t('模型 ID')} required value={config.model}
            spellCheck={false} onChange={event => update({model:event.target.value})}/></label>
          <label>{t('API 地址')}<input required type="url" value={config.baseUrl} spellCheck={false} autoCapitalize="none"
            onChange={event => {setRevealed(''); update({baseUrl:event.target.value, useSavedKey:false, apiKey:''});}}/></label>
          <label>API Key<span className="registry-key">
            <input aria-label="API Key" type={showKey ? 'text' : 'password'} autoComplete="off" spellCheck={false} autoCapitalize="none"
              value={config.useSavedKey ? (showKey ? revealed : '') : config.apiKey}
              onChange={event => update({apiKey:event.target.value, useSavedKey:false})}
              placeholder={t(config.useSavedKey ? '已保存' : '填写 API Key')}/>
            <button type="button" aria-label={t(showKey ? '隐藏密钥' : '显示密钥')} onClick={() => void reveal()}>
              {showKey ? <EyeOff size={16}/> : <Eye size={16}/>}
            </button>
          </span></label>
          <details className="registry-advanced">
            <summary>{t('高级')}<ChevronDown size={13}/></summary>
            <label>{t('API 协议')}<SelectField withinWindow aria-label={t('API 协议')} value={config.protocol}
              onValueChange={protocol => update({protocol, useSavedKey:false, apiKey:config.useSavedKey ? revealed : config.apiKey})}
              options={[
                {value:'responses',label:'Responses'},
                {value:'chat-completions',label:'Chat Completions'},
                {value:'anthropic-messages',label:'Anthropic Messages'},
              ]}/></label>
            <label>{t('代理地址')}<input type="url" value={config.proxyUrl} placeholder={t('直连')}
              onChange={event => update({proxyUrl:event.target.value})}/></label>
          </details>
        </fieldset>
      </div>
      <footer className="registry-save setup-footer">
        <Button variant="ghost" type="button" disabled={!!busy || keyBusy} onClick={onClose}>{t('取消')}</Button>
        <Button type="submit" disabled={!!busy || keyBusy}>
          {busy && <LoadingOrb size={15} />}{t(busy === 'test' ? '验证连接…' : busy ? '保存中…' : '保存')}
        </Button>
      </footer>
    </form>
    {agent && !onboarding && <>
      <div className="registry-scoped" hidden={section !== 'connections'}><AgentConnections agentId={agent.id} open={open && section === 'connections'}/></div>
      <div className="registry-scoped" hidden={section !== 'memory'}><AgentMemory agentId={agent.id} open={open && section === 'memory'}/></div>
      <div className="registry-scoped registry-voice-panel" hidden={section !== 'voice'}><AgentVoice agentId={agent.id} open={open && section === 'voice'}/></div>
    </>}
    {section !== 'config' && <footer className="registry-save setup-footer"><Button variant="ghost" type="button" onClick={onClose}>{t('返回')}</Button></footer>}
  </section>;
}
