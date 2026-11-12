'use client';
import {useCallback, useEffect, useLayoutEffect, useRef, useState} from 'react';
import {
  AgentSaveRequestSchema, ConnectionSaveRequestSchema, ConnectionTestRequestSchema,
  DeviceSaveRequestSchema, RegistryRequestSchema, RegistryStateSchema,
} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {ModelTestResultSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
import {DeviceRequestSchema, DeviceObservationSchema} from '@ai-phone/protocol/ai_phone/v1/phone_runtime_pb';
import {DesktopRequestSchema, DesktopObservationSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import type {MessageShape} from '@bufbuild/protobuf';
import {clientId} from '@/lib/client-id';
import {hasDeviceFrame, modelIssue, setupProgress, verifiedModels} from '@/lib/registry-setup';
import {settingsRpc} from '@/lib/settings-client';
import {useI18n} from './locale-provider';
import {FloatingWindow} from './floating-window';
import {RegistryAdministration} from './registry-administration';
import {DeviceModelMatching} from './registry-matching';
import {RegistryModels, modelDraft, type ModelEditor} from './registry-models';
import {RegistrySteps, SetupReview, type RegistryScreen} from './registry-onboarding';
import {Button} from './ui/button';
import './registry-window.css';

export type Registry = MessageShape<typeof RegistryStateSchema>;
type AgentDraft = {id:string; revision:bigint; name:string; modelId:string; deviceIds:string[]; instructions:string};
type DeviceDraft = {id:string; revision:bigint; name:string; kind:string; defaultAgentId:string};
const readRegistry = () => settingsRpc('state', RegistryRequestSchema, {}, RegistryStateSchema, 'registry');
const saveAgent = ({id, revision, name, modelId, deviceIds, instructions}:AgentDraft) =>
  settingsRpc('agent-save', AgentSaveRequestSchema,
    {id, expectedRevision:revision, name, modelConnectionId:modelId, deviceIds, instructions}, RegistryStateSchema, 'registry');
const saveDevice = ({id, revision, name, kind, defaultAgentId}:DeviceDraft) =>
  settingsRpc('device-save', DeviceSaveRequestSchema,
    {id, expectedRevision:revision, name, kind, defaultAgentId}, RegistryStateSchema, 'registry');

export function RegistryWindow({open, onClose, onNeedsSetup, onChange, activeDeviceId, voiceAgentId = ''}: {
  open:boolean; onClose:()=>void; onNeedsSetup:()=>void; onChange:(state:Registry)=>void; activeDeviceId:string; voiceAgentId?:string;
}) {
  const {t} = useI18n();
  const [state, setState] = useState<Registry | null>(null);
  const [screen, setScreen] = useState<RegistryScreen>('models');
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
          setScreen(progress.modelReady ? 'devices' : 'models');
          needsSetup.current();
        }
      }
    } catch (failure) {
      if (mounted.current && request === generation.current) setError(failure instanceof Error ? failure.message : t('读取失败'));
    }
  }, [accept, t]);
  useEffect(() => { void load(); }, [open, load]);
  useEffect(() => {
    if (open && voiceAgentId && !onboarding) setScreen('manage');
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

  async function saveModel(editor:ModelEditor) {
    const issue = modelIssue(editor.config);
    if (issue) { setError(t(issue)); return false; }
    const before = state?.models.find(model => model.id === editor.id);
    const changed = !before?.profile || JSON.stringify(modelDraft(before)) !== JSON.stringify(editor.config);
    return run(changed ? 'test' : 'save', async () => {
      let verificationId = '';
      if (changed) {
        const result = await settingsRpc('model-test', ConnectionTestRequestSchema,
          {id:editor.id, expectedRevision:editor.revision, draft:editor.config}, ModelTestResultSchema, 'registry');
        if (!result.ready) throw Error(result.checks.find(check => !check.passed)?.detail || t('连接检测失败'));
        verificationId = result.verificationId;
        if (mounted.current) setBusy('save');
      }
      const next = await settingsRpc('model-save', ConnectionSaveRequestSchema, {
        id:editor.id, expectedRevision:editor.revision, name:editor.name.trim() || editor.config.model.trim(),
        draft:changed ? editor.config : undefined, verificationId,
      }, RegistryStateSchema, 'registry');
      if (mounted.current) accept(next);
    });
  }

  async function assign(deviceId:string, modelId:string) {
    if (!state) return false;
    const snapshot = state;
    const device = snapshot.devices.find(row => row.id === deviceId);
    const model = verifiedModels(snapshot).find(row => row.id === modelId);
    if (!device || !model) return false;
    const current = snapshot.agents.find(agent => agent.id === device.defaultAgentId);
    if (current?.modelConnectionId === modelId) return true;
    return run(deviceId, async () => {
      const exclusive = (agent:Registry['agents'][number]) =>
        agent.deviceIds.every(id => id === deviceId) &&
        !snapshot.devices.some(row => row.id !== deviceId && row.defaultAgentId === agent.id);
      if (current && exclusive(current)) {
        const next = await saveAgent({
          id:current.id, revision:current.revision, name:current.name, modelId,
          deviceIds:[deviceId], instructions:current.instructions,
        });
        if (mounted.current) accept(next);
        return;
      }
      const reusable = snapshot.agents.find(agent =>
        agent.modelConnectionId === modelId && agent.deviceIds.includes(deviceId) && exclusive(agent));
      const agentId = reusable?.id ?? clientId();
      let next = snapshot;
      if (!reusable) {
        next = await saveAgent({id:agentId, revision:0n, name:model.name, modelId, deviceIds:[deviceId], instructions:''});
        if (mounted.current) accept(next);
      }
      const target = next.devices.find(row => row.id === deviceId)!;
      next = await saveDevice({...target, defaultAgentId:agentId});
      if (mounted.current) accept(next);
    });
  }

  async function finish() {
    await run('check', async () => {
      const next = await readRegistry();
      if (mounted.current) accept(next);
      const progress = setupProgress(next);
      if (!progress.ready) throw Error(t('请先完成 Agent 和设备配置'));
      await Promise.all(progress.deviceIds.map(async id => {
        const device = next.devices.find(row => row.id === id)!;
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
        setScreen('devices');
        onClose();
      }
    });
  }

  const models = state ? verifiedModels(state) : [];
  const progress = state && setupProgress(state);
  const footer = <RegistrySteps screen={screen} onboarding={onboarding} busy={!!busy} onScreen={go}/>;
  return <FloatingWindow id="configuration-center" title={t(onboarding ? '初次配置' : '配置中心')}
    open={open} onClose={onClose} centered={onboarding} focusPanel hideClose width={680} height={600}>
    <div className="registry-center" aria-busy={!!busy}>
      {!state ? <div className="registry-loading" role="status">{error || t('正在读取配置…')}</div> :
        <div className="registry-content" ref={body}>
          {screen === 'models' && <RegistryModels models={models} busy={busy} error={error} footer={footer}
            next={onboarding && <Button disabled={!models.length || !!busy} onClick={() => go('devices')}>{t('下一步')}</Button>}
            onSave={saveModel} onClearError={() => setError('')}/>}
          {screen === 'devices' && <DeviceModelMatching registry={state} models={models} busy={busy} error={error} footer={footer}
            next={onboarding && <Button disabled={!progress?.ready || !!busy} onClick={() => go('review')}>{t('下一步')}</Button>}
            onAssign={assign}/>}
          {screen === 'review' && <SetupReview state={state} busy={!!busy} error={error} steps={footer} onFinish={() => void finish()}/>}
          {screen === 'manage' && <RegistryAdministration registry={state} busy={busy} error={error} footer={footer} open={open}
            voiceAgentId={voiceAgentId} activeDeviceId={activeDeviceId} onRefresh={() => void load()}
            onSaveAgent={draft => run('save', async () => {const next = await saveAgent(draft); if (mounted.current) accept(next);})}
            onSaveDevice={draft => run('save', async () => {const next = await saveDevice(draft); if (mounted.current) accept(next);})}/>}
        </div>}
    </div>
  </FloatingWindow>;
}
