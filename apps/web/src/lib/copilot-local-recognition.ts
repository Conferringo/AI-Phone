export type LocalRecognitionEvent =
  | {type: 'speechstart'; at: number}
  | {type: 'transcript'; text: string; final: boolean; at: number}
  | {type: 'end'; at: number}
  | {type: 'error'; message: string; at: number};

export type LocalRecognitionAvailability = 'unsupported' | 'unavailable' | 'downloadable' | 'downloading' | 'available';

interface SpeechEngine extends EventTarget {
  processLocally: boolean;
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  start(): void;
  abort(): void;
}

type LocalOptions = {langs: string[]; processLocally: true; quality: 'dictation'};
interface SpeechEngineConstructor {
  new(): SpeechEngine;
  prototype: SpeechEngine;
  available(options: LocalOptions): Promise<Exclude<LocalRecognitionAvailability, 'unsupported'>>;
  install?(options: LocalOptions): Promise<boolean>;
}

function onDeviceConstructor() {
  const browser = globalThis as typeof globalThis & {
    SpeechRecognition?: SpeechEngineConstructor;
    webkitSpeechRecognition?: SpeechEngineConstructor;
  };
  const engine = browser.SpeechRecognition ?? browser.webkitSpeechRecognition;
  return engine && typeof engine.available === 'function' && 'processLocally' in engine.prototype
    ? engine
    : null;
}

function localOptions(language: string): LocalOptions {
  return {langs: [language], processLocally: true, quality: 'dictation'};
}

export function supportsLocalRecognition(): boolean {
  return onDeviceConstructor() !== null;
}

export async function localRecognitionAvailability(language: string): Promise<LocalRecognitionAvailability> {
  const engine = onDeviceConstructor();
  return engine ? engine.available(localOptions(language)) : 'unsupported';
}

export async function installLocalRecognition(language: string): Promise<boolean> {
  const engine = onDeviceConstructor();
  if (!engine?.install) throw Error('当前客户端无法安装本地语音语言包');
  return engine.install(localOptions(language));
}

const recognitionErrors: Partial<Record<string, string>> = {
  'audio-capture': '无法读取麦克风',
  'not-allowed': '麦克风权限未开启',
  'service-not-allowed': '当前客户端未启用本地语音识别',
  'language-not-supported': '当前语言的本地语音语言包不可用',
};

type SpeechResult = {isFinal: boolean; 0: {transcript: string}};
type RecognitionRun = {control: AbortController; engine?: SpeechEngine};

export class LocalRecognition {
  private current: RecognitionRun | null = null;

  async open(language: string, dispatch: (event: LocalRecognitionEvent) => void): Promise<void> {
    this.cancel();
    const run: RecognitionRun = {control: new AbortController()};
    this.current = run;
    const {signal} = run.control;
    try {
      const Engine = onDeviceConstructor();
      if (!Engine) throw Error('当前客户端不支持本地语音识别');
      const engine = new Engine();
      engine.processLocally = true;
      if (engine.processLocally !== true) throw Error('当前客户端无法启用本地语音识别');
      engine.lang = language;
      engine.continuous = false;
      engine.interimResults = true;
      run.engine = engine;
      await new Promise<void>((resolve, reject) => {
        let started = false;
        const release = () => {
          this.current = null;
          run.control.abort();
        };
        signal.addEventListener('abort', () => reject(signal.reason), {once: true});
        engine.addEventListener('start', () => {started = true; resolve();}, {once: true, signal});
        engine.addEventListener('speechstart', event => dispatch({type: 'speechstart', at: event.timeStamp}), {signal});
        engine.addEventListener('result', event => {
          const result = (event as Event & {results: ArrayLike<SpeechResult>}).results[0];
          dispatch({
            type: 'transcript',
            text: result?.[0].transcript.trim() ?? '',
            final: result?.isFinal ?? false,
            at: event.timeStamp,
          });
        }, {signal});
        engine.addEventListener('error', event => {
          const code = (event as Event & {error: string}).error;
          if (code === 'no-speech') return;
          const message = recognitionErrors[code] ?? '本地语音识别已停止';
          if (!started) reject(Error(message));
          release();
          if (started) dispatch({type: 'error', message, at: event.timeStamp});
        }, {signal});
        engine.addEventListener('end', event => {
          if (!started) reject(Error('本地语音识别未能启动'));
          release();
          if (started) dispatch({type: 'end', at: event.timeStamp});
        }, {signal});
        engine.start();
      });
    } catch (error) {
      if (this.current === run) this.cancel();
      throw error;
    }
  }

  cancel() {
    const run = this.current;
    this.current = null;
    run?.control.abort();
    run?.engine?.abort();
  }
}
