import {spawn} from 'node:child_process';
import {existsSync} from 'node:fs';

/** The guest owns ffmpeg; this connection renews its lease until disposal. */
export function guestCapture(kind:'screen'|'audio') {
  const child=spawn(existsSync('/usr/local/bin/docker')?'/usr/local/bin/docker':'docker',[
    'exec','-i','-u','ai','ai-phone-desktop','python3','/usr/local/lib/ai-phone/media-capture.py',kind,
  ],{stdio:['pipe','pipe','pipe']});
  let stopped=false;
  const heartbeat=setInterval(()=>{if(!stopped)child.stdin.write('1');},1000);
  heartbeat.unref();
  const clear=()=>{stopped=true;clearInterval(heartbeat);};
  child.stdin.on('error',clear);
  child.once('exit',clear);
  child.once('error',clear);
  child.stderr.on('data',()=>{});
  return {
    child,
    stop(){
      if(stopped)return;
      clear();
      // docker-exec SIGTERM does not terminate the guest. EOF releases its lease.
      child.stdin.end();
    },
  };
}
