'use client';
import { LoadingOrb } from "@/components/ui/loading-orb";
import {Monitor, Smartphone} from 'lucide-react';
import type {ReactNode} from 'react';
import {setupProgress} from '@/lib/registry-setup';
import {useI18n} from './locale-provider';
import {Button} from './ui/button';
import {AgentLabel} from './registry-agents';
import type {Registry} from './registry-window';

export type RegistryScreen = 'agents' | 'devices' | 'review';
const setupSteps = [['agents','Agent'], ['review','检查配置']] as const;
const settingsSteps = [['agents','Agent'], ['devices','设备']] as const;

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
  const progress = setupProgress(state);
  return <section className="registry-setup-stage" aria-labelledby="registry-review-heading">
    <h3 id="registry-review-heading" className="setup-title registry-setup-heading" data-setup-heading tabIndex={-1}>{t('检查配置')}</h3>
    <div className="registry-setup-scroll">
      <ul className="registry-setup-rows">
        {progress.agent && <li className="registry-setup-row">
          <AgentLabel agent={progress.agent} model={progress.model}/>
        </li>}
        {state.devices.map(device => {
          const Icon = device.kind === 'phone' ? Smartphone : Monitor;
          return <li className="registry-setup-row" key={device.id}>
            <span className="registry-setup-identity"><span aria-hidden="true"><Icon size={18}/></span>
              <span><strong>{device.name}</strong><small>{device.system.replace(' · 待安装', '')}</small></span>
            </span>
            {device.runtimeState !== 'managed' && <span className="registry-setup-status">{t('待安装')}</span>}
          </li>;
        })}
      </ul>
      {error && <p role="alert" className="registry-setup-error">{error}</p>}
    </div>
    <footer className="registry-save setup-footer">{steps}<Button disabled={busy || !progress.ready} onClick={onFinish}>
      {busy && <LoadingOrb size={15} />} {t(busy ? '检查设备…' : '检查并进入')}
    </Button></footer>
  </section>;
}
