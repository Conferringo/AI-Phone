/* Local microphone capture only. PCM never reaches a provider from this worklet. */
class CopilotCapture extends AudioWorkletProcessor {
  constructor(){super();this.frame=new Float32Array(Math.round(sampleRate/20));this.offset=0;}
  process(inputs,outputs){
    for(const output of outputs)for(const channel of output)channel.fill(0);
    const channels=inputs[0];if(!channels?.length)return true;
    for(let i=0;i<channels[0].length;i++){
      let value=0;for(const channel of channels)value+=channel[i];
      this.frame[this.offset++]=value/channels.length;
      if(this.offset===this.frame.length){
        this.port.postMessage({samples:this.frame,rate:sampleRate,atMs:(currentTime+(i+1)/sampleRate)*1000},[this.frame.buffer]);
        this.frame=new Float32Array(Math.round(sampleRate/20));this.offset=0;
      }
    }
    return true;
  }
}
registerProcessor('ai-phone-copilot',CopilotCapture);
