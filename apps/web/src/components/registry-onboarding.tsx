'use client';
import {LoaderCircle, Monitor, Smartphone} from 'lucide-react';
import type {ReactNode} from 'react';
import {setupProgress, verifiedModels} from '@/lib/registry-setup';
import {useI18n} from './locale-provider';
import {Button} from './ui/button';
import {ModelLabel} from './registry-models';
import type {Registry} from './registry-window';

export type RegistryScreen = 'models' | 'devices' | 'review' | 'manage';
const setupSteps = [['models','模型'], ['devices','设备'], ['review','检查配置']] as const;
const settingsSteps = [['models','模型'], ['devices','设备'], ['manage','管理']] as const;

export function RegistrySteps({screen, onboarding, busy, onScreen}: {
  screen:RegistryScreen; onboarding:boolean; busy:boolean; onScreen:(screen:RegistryScreen)=>void;
}) {
  const {t} = useI18n();
  const steps = onboarding ? setupSteps : settingsSteps;
  const current = steps.findIndex(([value]) => value === screen);
  return <nav aria-label={t(onboarding ? '初次配置' : '配置中心')}><ol className="registry-steps">
    {steps.map(([value, label], index) => <li key={value} aria-current={screen === value ? 'step' : undefined}>
      {screen !== value && (!onboarding || index < current)
        ? <button type="button" disabled={busy} onClick={() => onScreen(value)}>{t(label)}</button>
        : <span>{t(label)}</span>}
    </li>)}
  </ol></nav>;
}

export function SetupReview({state, busy, error, steps, onFinish}: {
  state:Registry; busy:boolean; error:string; steps:ReactNode; onFinish:()=>void;
}) {
  const {t} = useI18n();
  const progress = setupProgress(state), models = verifiedModels(state);
  return <section className="registry-setup-stage" aria-labelledby="registry-review-heading">
    <h3 id="registry-review-heading" className="setup-title registry-setup-heading" data-setup-heading tabIndex={-1}>{t('检查配置')}</h3>
    <div className="registry-setup-scroll">
      <ul className="registry-setup-rows">
        {state.devices.map(device => {
          const agent = state.agents.find(row => row.id === device.defaultAgentId);
          const model = models.find(row => row.id === agent?.modelConnectionId);
          const Icon = device.kind === 'phone' ? Smartphone : Monitor;
          return <li className="registry-setup-row" key={device.id}>
            <span className="registry-setup-identity"><span aria-hidden="true"><Icon size={18}/></span>
              <span><strong>{device.name}</strong><small>{device.system.replace(' · 待安装', '')}</small></span>
            </span>
            {model ? <ModelLabel model={model}/> :
              <span className="registry-setup-status">{t(device.runtimeState === 'managed' ? '待配置' : '待安装')}</span>}
          </li>;
        })}
      </ul>
      {error && <p role="alert" className="registry-setup-error">{error}</p>}
    </div>
    <footer className="registry-save setup-footer">{steps}<Button disabled={busy || !progress.ready} onClick={onFinish}>
      {busy && <LoaderCircle className="spinning" size={15}/>} {t(busy ? '检查设备…' : '检查并进入')}
    </Button></footer>
  </section>;
}
