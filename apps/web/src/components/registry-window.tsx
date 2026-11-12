'use client';
import { LoadingOrb } from "@/components/ui/loading-orb";
import {useCallback, useEffect, useLayoutEffect, useRef, useState} from 'react';
import {
  AgentConfigurationSaveRequestSchema, AgentProfileRequestSchema, ConnectionTestRequestSchema,
  DeviceSaveRequestSchema, RegistryRequestSchema, RegistryStateSchema,
} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {ModelTestResultSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
import {DeviceRequestSchema, DeviceObservationSchema} from '@ai-phone/protocol/ai_phone/v1/phone_runtime_pb';
import {DesktopRequestSchema, DesktopObservationSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import type {MessageShape} from '@bufbuild/protobuf';
import {hasDeviceFrame, modelIssue, setupProgress} from '@/lib/registry-setup';
import {settingsRpc} from '@/lib/settings-client';
import {useI18n} from './locale-provider';
import {FloatingWindow} from './floating-window';
import {RegistryAdministration, type DeviceDraft} from './registry-administration';
import {RegistryAgents, modelDraft, type AgentEditor} from './registry-agents';
import {RegistrySteps, SetupReview, type RegistryScreen} from './registry-onboarding';
import {Button} from './ui/button';
import './registry-window.css';

export type Registry = MessageShape<typeof RegistryStateSchema>;
const readRegistry = () => settingsRpc('state', RegistryRequestSchema, {}, RegistryStateSchema, 'registry');

export function RegistryWindow({open, onClose, onNeedsSetup, onChange, activeDeviceId, voiceAgentId = ''}: {
  open:boolean; onClose:()=>void; onNeedsSetup:()=>void; onChange:(state:Registry)=>void; activeDeviceId:string; voiceAgentId?:string;
}) {
  const {t} = useI18n();
  const [state, setState] = useState<Registry | null>(null);
  const [screen, setScreen] = useState<RegistryScreen>('agents');
  const [onboarding, setOnboarding] = useState(false);
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');
  const pending = useRef(false), initialized = useRef(false), mounted = useRef(true), generation = useRef(0);
  const body = useRef<HTMLDivElement>(null), change = useRef(onChange), needsSetup = useRef(onNeedsSetup);
  change.current = onChange;
  needsSetup.current = onNeedsSetup;
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; generation.current++; };
  }, []);

  const accept = useCallback((next:Registry) => {
    setState(next);
    change.current(next);
  }, []);
  const load = useCallback(async () => {
    if (pending.current) return;
    const request = ++generation.current;
    try {
      const next = await readRegistry();
      if (!mounted.current || request !== generation.current) return;
      accept(next);
      setError('');
      if (!initialized.current) {
        initialized.current = true;
        const progress = setupProgress(next);
        if (!progress.ready) {
          setOnboarding(true);
          setScreen('agents');
          needsSetup.current();
        }
      }
    } catch (failure) {
      if (mounted.current && request === generation.current) setError(failure instanceof Error ? failure.message : t('读取失败'));
    }
  }, [accept, t]);
  useEffect(() => { void load(); }, [open, load]);
  useEffect(() => {
    if (open && voiceAgentId && !onboarding) setScreen('agents');
  }, [open, voiceAgentId, onboarding]);
  useLayoutEffect(() => {
    if (open) body.current?.querySelector<HTMLElement>('[data-setup-heading]')?.focus({preventScroll:true});
  }, [open, screen, state === null]);

  function go(next:RegistryScreen) {
    if (pending.current) return;
    setError('');
    setScreen(next);
  }

  async function run(action:string, work:()=>Promise<void>) {
    if (pending.current) return false;
    pending.current = true;
    generation.current++;
    setBusy(action);
    setError('');
    try {
      await work();
      return true;
    } catch (failure) {
      if (mounted.current) setError(failure instanceof Error ? failure.message : t('保存失败'));
      return false;
    } finally {
      pending.current = false;
      if (mounted.current) setBusy('');
    }
  }

  async function saveAgent({id, agent, modelId, model, name, config}:AgentEditor) {
    const issue = modelIssue(config);
    if (issue) { setError(t(issue)); return false; }
    const changed = !model?.profile || JSON.stringify(modelDraft(model)) !== JSON.stringify(config);
    const label = name.trim() || config.model.trim();
    const expectedRevision = model?.revision ?? 0n;
    return run(changed ? 'test' : 'save', async () => {
      let verificationId = '';
      if (changed) {
        const result = await settingsRpc('model-test', ConnectionTestRequestSchema,
          {id:modelId, expectedRevision, draft:config}, ModelTestResultSchema, 'registry');
        if (!result.ready) throw Error(result.checks.find(check => !check.passed)?.detail || t('连接检测失败'));
        verificationId = result.verificationId;
        if (mounted.current) setBusy('save');
      }
      const next = await settingsRpc('agent-config-save', AgentConfigurationSaveRequestSchema, {
        agent: {id, expectedRevision:agent?.revision ?? 0n, name:label, modelConnectionId:modelId, instructions:agent?.instructions ?? ''},
        connection: changed ? {id:modelId, expectedRevision, name:label, draft:config, verificationId} : undefined,
      }, RegistryStateSchema, 'registry');
      if (mounted.current) accept(next);
    });
  }

  function profileAction(operation:'agent-select' | 'agent-delete', agent:Registry['agents'][number]) {
    return run(operation, async () => {
      const next = await settingsRpc(operation, AgentProfileRequestSchema,
        {id:agent.id, expectedRevision:agent.revision}, RegistryStateSchema, 'registry');
      if (mounted.current) accept(next);
    });
  }

  function saveDevice({id, revision, name, kind}:DeviceDraft) {
    return run('save', async () => {
      const next = await settingsRpc('device-save', DeviceSaveRequestSchema,
        {id, expectedRevision:revision, name, kind}, RegistryStateSchema, 'registry');
      if (mounted.current) accept(next);
    });
  }

  async function finish() {
    await run('check', async () => {
      const next = await readRegistry();
      if (mounted.current) accept(next);
      const progress = setupProgress(next);
      if (!progress.ready) throw Error(t('请先完成 Agent 和设备配置'));
      await Promise.all(progress.devices.map(async device => {
        const id = device.id;
        try {
          const frame = device.kind === 'phone'
            ? await settingsRpc('observe', DeviceRequestSchema,
              {phoneId:id, ownerId:'local-user', includeElements:true}, DeviceObservationSchema, 'phone')
            : await settingsRpc('observe', DesktopRequestSchema, {deviceId:id}, DesktopObservationSchema, 'desktop');
          if (!hasDeviceFrame(id, frame)) throw Error(t('未收到设备画面'));
        } catch (failure) {
          throw Error(`${device.name} · ${failure instanceof Error ? failure.message : t('连接失败')}`);
        }
      }));
      if (mounted.current) {
        setOnboarding(false);
        setScreen('agents');
        onClose();
      }
    });
  }

  const progress = state && setupProgress(state);
  const footer = <RegistrySteps screen={screen} onboarding={onboarding} busy={!!busy} onScreen={go}/>;
  return <FloatingWindow id="configuration-center" title={t(onboarding ? '初次配置' : '配置中心')}
    open={open} onClose={onClose} centered={onboarding} focusPanel hideClose width={680} height={600}>
    <div className="registry-center" aria-busy={!!busy}>
      {!state ? <div className="registry-loading" role={error ? 'alert' : 'status'}>
        <span>{!error && <LoadingOrb size={20} />}{error || t('正在读取配置…')}</span>
      </div> :
        <div className="registry-content" ref={body}>
          {screen === 'agents' && <RegistryAgents registry={state} onboarding={onboarding} open={open}
            voiceAgentId={onboarding ? '' : voiceAgentId} busy={busy} error={error} footer={footer}
            next={onboarding && <Button disabled={!progress?.ready || !!busy} onClick={() => go('review')}>{t('下一步')}</Button>}
            onSave={saveAgent} onSelect={agent => profileAction('agent-select', agent)}
            onDelete={agent => profileAction('agent-delete', agent)} onClearError={() => setError('')}/>}
          {screen === 'devices' && <RegistryAdministration registry={state} busy={busy} error={error} footer={footer} open={open}
            activeDeviceId={activeDeviceId} onRefresh={() => void load()} onSave={saveDevice}/>}
          {screen === 'review' && <SetupReview state={state} busy={!!busy} error={error} steps={footer} onFinish={() => void finish()}/>}
        </div>}
    </div>
  </FloatingWindow>;
}
