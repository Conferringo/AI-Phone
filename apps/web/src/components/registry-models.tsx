'use client';
import {create, type MessageShape} from '@bufbuild/protobuf';
import {ModelDraftSchema, ModelCredentialSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
import {ConnectionCredentialRequestSchema, type RegisteredModelSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {ChevronDown, Eye, EyeOff, LoaderCircle, Plus} from 'lucide-react';
import {useEffect, useRef, useState, type ReactNode} from 'react';
import {clientId} from '@/lib/client-id';
import {modelPreset, modelPresets, presetDraft} from '@/lib/registry-setup';
import {settingsRpc} from '@/lib/settings-client';
import {useI18n} from './locale-provider';
import {Button} from './ui/button';
import {SelectField} from './ui/select';

export type RegisteredModel = MessageShape<typeof RegisteredModelSchema>;
type Config = MessageShape<typeof ModelDraftSchema>;
export type ModelEditor = {
  id: string;
  revision: bigint;
  name: string;
  config: Config;
};

export function modelDraft(model?: RegisteredModel): Config {
  if (!model?.profile) return presetDraft('deepseek');
  const {provider, protocol, baseUrl, model: modelId, proxyUrl, hasApiKey} = model.profile;
  return create(ModelDraftSchema, {provider, protocol, baseUrl, model: modelId, proxyUrl, useSavedKey: hasApiKey});
}

export function ModelLabel({model}: {model: RegisteredModel}) {
  const profile = model.profile;
  const presetId = profile && modelPreset(profile);
  const preset = modelPresets.find(option => option.id !== 'custom' && option.id === presetId);
  const detail = [preset?.label, profile?.model !== model.name ? profile?.model : ''].filter(Boolean).join(' · ');
  return <span className="registry-model-label">
    <strong>{model.name}</strong>
    {detail && <small>{detail}</small>}
  </span>;
}

export function RegistryModels({models, busy, error, footer, next, onSave, onClearError}: {
  models: RegisteredModel[];
  busy: string;
  error: string;
  footer: ReactNode;
  next?: ReactNode;
  onSave: (draft: ModelEditor) => Promise<boolean>;
  onClearError: () => void;
}) {
  const {t} = useI18n();
  const [editor, setEditor] = useState<ModelEditor | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [revealed, setRevealed] = useState('');
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState('');
  const nameField = useRef<HTMLInputElement>(null);
  const editingId = editor?.id;
  useEffect(() => { if (editingId) nameField.current?.focus({preventScroll:true}); }, [editingId]);

  function edit(model?: RegisteredModel) {
    onClearError();
    setEditor({
      id: model?.id ?? clientId(),
      revision: model?.revision ?? 0n,
      name: model?.name ?? '',
      config: modelDraft(model),
    });
    setShowKey(false);
    setRevealed('');
    setKeyError('');
  }
  function closeEditor() {
    setEditor(null);
    setRevealed('');
    setKeyError('');
    onClearError();
  }
  function update(patch: Partial<Config>) {
    setEditor(current => current && {...current, config: {...current.config, ...patch}});
    onClearError();
  }
  async function reveal() {
    if (!editor) return;
    if (showKey || !editor.config.useSavedKey || revealed) {
      setShowKey(!showKey);
      return;
    }
    setKeyBusy(true);
    setKeyError('');
    try {
      const credential = await settingsRpc('credential', ConnectionCredentialRequestSchema,
        {id: editor.id, expectedRevision: editor.revision}, ModelCredentialSchema, 'registry');
      setRevealed(credential.apiKey);
      setShowKey(true);
    } catch (failure) {
      setKeyError(failure instanceof Error ? failure.message : t('读取密钥失败'));
    } finally {
      setKeyBusy(false);
    }
  }
  if (!editor) return <section className="registry-setup-stage" aria-label={t('模型')}>
    <div className="registry-pane-heading">
      <h3 className="setup-title" data-setup-heading tabIndex={-1}>{t('模型')}</h3>
      <Button type="button" variant="outline" size="icon" className="registry-model-add" aria-label={t('添加模型')} title={t('添加模型')} disabled={!!busy} onClick={() => edit()}>
        <Plus size={19}/>
      </Button>
    </div>
    <div className="registry-setup-scroll">
      {error && <p role="alert" className="registry-setup-error">{error}</p>}
      {models.length ? <ul className="registry-setup-rows registry-model-list">
        {models.map(model => <li key={model.id}>
          <button type="button" disabled={!!busy} onClick={() => edit(model)}>
            <ModelLabel model={model}/>
          </button>
        </li>)}
      </ul> : <p className="registry-setup-empty">{t('暂无模型')}</p>}
    </div>
    <footer className="registry-save setup-footer">{footer}{next}</footer>
  </section>;

  const config = editor.config;
  return <form className="registry-form registry-model-editor" onSubmit={async event => {
    event.preventDefault();
    if (await onSave(editor)) closeEditor();
  }}>
    <h3 className="setup-title registry-setup-heading" data-setup-heading tabIndex={-1}>
      {t(editor.revision ? '编辑模型' : '添加模型')}
    </h3>
    <div className="registry-scroll">
      {(error || keyError) && <p role="alert" className="registry-setup-error">{error || keyError}</p>}
      <fieldset disabled={!!busy || keyBusy}>
        <label>{t('名称')}<input ref={nameField} aria-label={t('名称')} maxLength={80} placeholder={config.model}
          value={editor.name} onChange={event => {setEditor({...editor, name: event.target.value});onClearError();}}/></label>
        <label>{t('服务商')}<SelectField withinWindow aria-label={t('服务商')} value={modelPreset(config)}
          onValueChange={value => {
            setRevealed('');
            setShowKey(false);
            update(presetDraft(value));
          }}
          options={modelPresets.map(preset => ({value:preset.id, label:preset.id === 'custom' ? t('兼容服务') : preset.label}))}/></label>
        <label>{t('模型')}<input aria-label={t('模型 ID')} required value={config.model}
          spellCheck={false} onChange={event => update({model: event.target.value})}/></label>
        <label>{t('API 地址')}<input aria-label={t('API 地址')} required type="url" value={config.baseUrl}
          spellCheck={false} autoCapitalize="none" onChange={event => {
            setRevealed('');
            update({baseUrl:event.target.value, useSavedKey:false, apiKey:''});
          }}/></label>
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
          <label>{t('代理地址')}<input aria-label={t('代理地址')} type="url" value={config.proxyUrl}
            placeholder={t('直连')} onChange={event => update({proxyUrl:event.target.value})}/></label>
        </details>
      </fieldset>
    </div>
    <footer className="registry-save setup-footer">
      <Button variant="ghost" type="button" disabled={!!busy || keyBusy} onClick={closeEditor}>{t('取消')}</Button>
      <Button type="submit" disabled={!!busy || keyBusy}>
        {busy && <LoaderCircle className="spinning" size={15}/>}
        {t(busy === 'test' ? '验证连接…' : busy ? '保存中…' : '保存')}
      </Button>
    </footer>
  </form>;
}
