"use client";

import {useEffect, useRef, useState, type ImgHTMLAttributes} from 'react';
import {clientId} from '@/lib/client-id';

// Decode independently of the displayed <img>. A missed load event must never
// leave the presentation queue locked while new network frames keep arriving.
export function PhoneStreamImage({fallback, enabled, onConnectionState, endpoint='/api/phone/media/screen', ...props}: ImgHTMLAttributes<HTMLImageElement> & {
  fallback: string;
  endpoint?: string;
  enabled: boolean;
  onConnectionState: (online: boolean) => void;
}) {
  const [url, setUrl] = useState('');
  const connection = useRef(onConnectionState);
  const presented = useRef<(source: string) => void>(() => {});
  connection.current = onConnectionState;

  useEffect(() => {
    connection.current(false);
    setUrl('');
    if (!enabled) return;
    let live = true;
    let abort: AbortController | undefined;
    const owned = new Set<string>();
    const release = (source: string) => {
      URL.revokeObjectURL(source);
      owned.delete(source);
    };
    const reconnect = () => abort?.abort();
    let reportedAt = -Infinity;
    const diagnose = (reason:string) => {
      if(performance.now()-reportedAt<10000)return;
      reportedAt=performance.now();
      // Codes only: no frame content, device IDs, credentials or conversations.
      console.warn('aiphone-screen',endpoint.split('?')[0],reason);
    };
    window.addEventListener('online', reconnect);

    void (async () => {
      while (live) {
        abort = new AbortController();
        let active = true;
        let pending: Uint8Array | null = null;
        let decoding = false;
        let cancelDecode: (() => void) | undefined;
        let expectedUrl = '';
        let decodingUrl = '';
        let lastPacket = performance.now();
        let lastPresented = performance.now();
        let waitingForPresentation = false;
        presented.current = source => {
          if (!active || source !== expectedUrl) return;
          lastPresented = performance.now();
          waitingForPresentation = false;
          connection.current(true);
          for (const old of owned) if (old !== source && old !== decodingUrl && old !== expectedUrl) release(old);
          void pump();
        };
        const pump = async () => {
          if (decoding || waitingForPresentation) return;
          decoding = true;
          try {
            // Keep one frame in presentation. Replacing src again before its
            // load event can starve WebKit on larger desktop frames forever.
            // Incoming packets overwrite pending, so no stale-frame queue grows.
            while (active && live && pending && !waitingForPresentation) {
              const bytes = pending;
              pending = null;
              const source = URL.createObjectURL(new Blob([new Uint8Array(bytes)], {type: 'image/jpeg'}));
              owned.add(source);
              decodingUrl = source;
              const image = new Image();
              try {
                await new Promise<void>((resolve, reject) => {
                  const finish = (error?: Error) => {
                    clearTimeout(timer);
                    image.onload = image.onerror = null;
                    cancelDecode = undefined;
                    if (error) reject(error); else resolve();
                  };
                  const timer = setTimeout(() => finish(new Error('frame_decode_timeout')), 2000);
                  cancelDecode = () => finish(new Error('stream_closed'));
                  image.onload = () => finish();
                  image.onerror = () => finish(new Error('invalid_frame'));
                  image.src = source;
                });
                if (!active || !live) { release(source); break; }
                expectedUrl = source;
                waitingForPresentation = true;
                setUrl(source);
                // Keep the current and previous URLs until the new image is
                // actually presented. Bound memory even if the browser stalls.
                const previous = [...owned];
                for (const old of previous.slice(0, -2)) release(old);
              } catch (error) {
                diagnose(error instanceof Error?error.message:'frame_decode_failed');
                release(source);
                if (active && live) abort?.abort();
                break;
              }
            }
          } finally {
            decoding = false;
            decodingUrl = '';
            if(active && live && pending && !waitingForPresentation)void pump();
          }
        };
        const watchdog = setInterval(() => {
          const now = performance.now();
          if (now - lastPacket > 4000 || (waitingForPresentation && now - lastPresented > 3000)) {
            diagnose(now-lastPacket>4000?'network_timeout':'presentation_timeout');reconnect();
          }
        }, 500);
        try {
          // WebKit can consume multipart responses before exposing fetch chunks.
          // The BFF keeps identical framing but serves this parser as raw bytes.
          const response = await fetch(endpoint + (endpoint.includes('?')?'&':'?')+'raw=1&v=' + clientId(), {cache: 'no-store', signal: abort.signal});
          if (!response.ok || !response.body) throw new Error('screen_http_'+response.status);
          const reader = response.body.getReader();
          let data = new Uint8Array();
          while (active && live) {
            const next = await reader.read();
            if (!live || next.done) break;
            lastPacket = performance.now();
            const combined = new Uint8Array(data.length + next.value.length);
            combined.set(data);
            combined.set(next.value, data.length);
            data = combined;
            if (data.length > 2_000_000) throw new Error('frame_limit');
            for (;;) {
              let end = -1;
              for (let i = 0; i + 3 < data.length; i++) {
                if (data[i] === 13 && data[i + 1] === 10 && data[i + 2] === 13 && data[i + 3] === 10) { end = i; break; }
              }
              if (end < 0) break;
              const header = new TextDecoder().decode(data.subarray(0, end));
              const length = Number(header.match(/Content-Length:\s*(\d+)/i)?.[1]);
              if (!length || length > 1_000_000) throw new Error('frame_header');
              if (data.length < end + 4 + length) break;
              pending = data.slice(end + 4, end + 4 + length);
              data = data.slice(end + 4 + length);
            }
            void pump();
          }
        } catch (error) {
          if(error instanceof Error&&error.name!=='AbortError')diagnose(error.message);
          connection.current(false);
        } finally {
          active = false;
          pending = null;
          cancelDecode?.();
          clearInterval(watchdog);
          abort.abort();
          connection.current(false);
          if (live) setUrl('');
          for (const source of owned) release(source);
        }
        if (live) await new Promise(resolve => setTimeout(resolve, 250));
      }
    })();
    return () => {
      live = false;
      abort?.abort();
      window.removeEventListener('online', reconnect);
      presented.current = () => {};
      connection.current(false);
      for (const source of owned) release(source);
    };
  }, [enabled,endpoint]);

  return <img {...props} src={enabled && url ? url : fallback || undefined}
    onLoad={event => { presented.current(event.currentTarget.currentSrc); props.onLoad?.(event); }}
    onError={event => { connection.current(false); props.onError?.(event); }}/>
}
