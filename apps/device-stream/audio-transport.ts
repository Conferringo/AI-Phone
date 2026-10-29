import type {ServerResponse} from 'node:http';

/** A single silent stereo frame flushes streaming proxies before a quiet guest plays.
 * It is transport pre-roll, not evidence that the guest has produced audio.
 */
export function beginAudio(res:ServerResponse){
 res.writeHead(200,{'content-type':'audio/L16;rate=48000;channels=2','cache-control':'private,no-store','x-sample-format':'s16le'});
 res.flushHeaders();res.write(Buffer.alloc(4));
}
