export function localVoice(language:string):SpeechSynthesisVoice|undefined{
  const voices=globalThis.speechSynthesis?.getVoices().filter(voice=>voice.localService)??[];
  const base=language.split('-')[0].toLowerCase();
  return voices.find(voice=>voice.lang.toLowerCase()===language.toLowerCase())
    ??voices.find(voice=>voice.lang.toLowerCase().split(/[-_]/)[0]===base);
}

export function speakLocally(text:string,voice:SpeechSynthesisVoice,signal:AbortSignal):Promise<void>{
  if(signal.aborted)return Promise.resolve();
  return new Promise<void>((resolve,reject)=>{
    const utterance=new SpeechSynthesisUtterance(text);
    utterance.voice=voice;
    utterance.lang=voice.lang;
    const complete=(error?:Error)=>{
      signal.removeEventListener('abort',cancel);
      utterance.onend=null;
      utterance.onerror=null;
      if(error)reject(error);else resolve();
    };
    const cancel=()=>{complete();speechSynthesis.cancel();};
    utterance.onend=()=>complete();
    utterance.onerror=()=>complete(Error('本机播报失败'));
    signal.addEventListener('abort',cancel,{once:true});
    speechSynthesis.speak(utterance);
  });
}
