'use client';
import { LoadingOrb } from "@/components/ui/loading-orb";
import {GripVertical, Monitor, Smartphone} from 'lucide-react';
import {useRef, useState, type ReactNode} from 'react';
import {useI18n} from './locale-provider';
import {ModelLabel, type RegisteredModel} from './registry-models';
import type {Registry} from './registry-window';

type Selection = {kind: 'device' | 'model'; id: string};

export function DeviceModelMatching({registry, models, busy, error, footer, next, onAssign}: {
  registry: Registry;
  models: RegisteredModel[];
  busy: string;
  error: string;
  footer: ReactNode;
  next?: ReactNode;
  onAssign: (deviceId: string, modelId: string) => Promise<boolean>;
}) {
  const {t} = useI18n();
  const [selection, setSelection] = useState<Selection | null>(null);
  const [hovered, setHovered] = useState('');
  const [settled, setSettled] = useState('');
  const dragged = useRef('');

  function clearDrag() {
    dragged.current = '';
    setHovered('');
  }
  async function assign(deviceId: string, modelId: string) {
    setSelection(null);
    setSettled('');
    if (await onAssign(deviceId, modelId)) setSettled(deviceId);
  }
  function select(kind: Selection['kind'], id: string) {
    if (selection && selection.kind !== kind) {
      if (kind === 'device') void assign(id, selection.id);
      else void assign(selection.id, id);
      return;
    }
    setSelection(selection?.id === id ? null : {kind, id});
  }

  return <section className="registry-setup-stage" aria-label={t('设备')}>
    <h3 className="setup-title registry-setup-heading" data-setup-heading tabIndex={-1}>{t('设备')}</h3>
    <div className="registry-setup-scroll">
      <ul className="registry-setup-rows">
        {registry.devices.map(device => {
          const assigned = registry.agents.find(agent => agent.id === device.defaultAgentId);
          const model = models.find(candidate => candidate.id === assigned?.modelConnectionId);
          const selected = selection?.kind === 'device' && selection.id === device.id;
          const targeted = selected || hovered === device.id;
          const Icon = device.kind === 'phone' ? Smartphone : Monitor;
          return <li key={device.id} className="registry-setup-row registry-match-row">
            <span className="registry-setup-identity"><span aria-hidden="true"><Icon size={18}/></span>
              <span><strong>{device.name}</strong><small>{device.system.replace(' · 待安装', '')}</small></span>
            </span>
            <div className="registry-drop-zone"
              onDragOver={event => {
                if (!dragged.current || busy) return;
                event.preventDefault();
                event.dataTransfer.dropEffect = 'copy';
                if (hovered !== device.id) setHovered(device.id);
              }}
              onDragLeave={event => {
                const next = event.relatedTarget;
                if (!(next instanceof Node && event.currentTarget.contains(next))) setHovered('');
              }}
              onDrop={event => {
                const modelId = dragged.current;
                if (!modelId || busy) return;
                event.preventDefault();
                clearDrag();
                void assign(device.id, modelId);
              }}>
              <button type="button" disabled={!!busy} aria-pressed={selected}
                aria-label={t('为 {0} 选择模型', {'0':device.name})}
                className="registry-drop-slot" data-targeted={targeted}
                data-awaiting={selection?.kind === 'model'} data-filled={!!model}
                data-settled={settled === device.id} onClick={() => select('device', device.id)}>
                {busy === device.id ? <LoadingOrb size={16} /> :
                  model ? <ModelLabel model={model}/> : <span>{t('未分配')}</span>}
              </button>
            </div>
          </li>;
        })}
      </ul>
      <div className="registry-match-library">
        <h4>{t('模型')}</h4>
        {models.length ? <ul className="registry-match-models">
          {models.map(model => <li key={model.id}>
            <button type="button" className="registry-model-token" disabled={!!busy} draggable={!busy}
              aria-pressed={selection?.kind === 'model' && selection.id === model.id}
              onClick={() => select('model', model.id)}
              onDragStart={event => {
                dragged.current = model.id;
                setSelection(null);
                event.dataTransfer.effectAllowed = 'copy';
                event.dataTransfer.setData('text/plain', model.id);
              }}
              onDragEnd={clearDrag}>
              <GripVertical size={14} aria-hidden="true"/><ModelLabel model={model}/>
            </button>
          </li>)}
        </ul> : <p className="registry-setup-empty">{t('暂无模型')}</p>}
      </div>
      {error && <p role="alert" className="registry-setup-error">{error}</p>}
    </div>
    <footer className="registry-save setup-footer">{footer}{next}</footer>
  </section>;
}
