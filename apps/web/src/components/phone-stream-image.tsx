"use client";

import {useEffect, useRef, useState, type ImgHTMLAttributes} from 'react';
import {clientId} from '@/lib/client-id';
import {ScreenFrames} from '@/lib/screen-frames';

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
  const presented = useRef<(source: string, loaded: boolean) => void>(() => {});
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
        let pending: Uint8Array<ArrayBuffer> | null = null;
        let decoding = false;
        let cancelDecode: (() => void) | undefined;
        let expectedUrl = '';
        let decodingUrl = '';
        let lastFrame = performance.now();
        let presentationStarted: number | null = null;
        presented.current = (source, loaded) => {
          if (!active || !live || presentationStarted === null || source !== expectedUrl) return;
          if (!loaded) {
            reconnect();
            return;
          }
          presentationStarted = null;
          connection.current(true);
          for (const old of owned) if (old !== source && old !== decodingUrl) release(old);
          void pump();
        };
        const pump = async () => {
          if (decoding || presentationStarted !== null) return;
          decoding = true;
          try {
            // Keep one frame in presentation. Replacing src again before its
            // load event can starve WebKit on larger desktop frames forever.
            // Incoming packets overwrite pending, so no stale-frame queue grows.
            while (active && live && pending && presentationStarted === null) {
              const bytes = pending;
              pending = null;
              const source = URL.createObjectURL(new Blob([bytes], {type: 'image/jpeg'}));
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
                presentationStarted = performance.now();
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
            if (active && live && pending && presentationStarted === null) void pump();
          }
        };
        const watchdog = setInterval(() => {
          const now = performance.now();
          const frameTimeout = now - lastFrame > 4000;
          if (frameTimeout || (presentationStarted !== null && now - presentationStarted > 3000)) {
            diagnose(frameTimeout ? 'frame_timeout' : 'presentation_timeout');
            reconnect();
          }
        }, 500);
        try {
          // WebKit can consume multipart responses before exposing fetch chunks.
          // The BFF keeps identical framing but serves this parser as raw bytes.
          const response = await fetch(endpoint + (endpoint.includes('?')?'&':'?')+'raw=1&v=' + clientId(), {cache: 'no-store', signal: abort.signal});
          if (!response.ok || !response.body) throw new Error('screen_http_'+response.status);
          const reader = response.body.getReader();
          const frames = new ScreenFrames();
          while (active && live) {
            const next = await reader.read();
            if (!live || next.done) break;
            const frame = frames.push(next.value);
            if (frame) {
              lastFrame = performance.now();
              pending = frame;
            }
            void pump();
          }
        } catch (error) {
          if(error instanceof Error&&error.name!=='AbortError')diagnose(error.message);
        } finally {
          active = false;
          pending = null;
          cancelDecode?.();
          clearInterval(watchdog);
          abort.abort();
          if (live) {
            connection.current(false);
            setUrl('');
          }
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
    onLoad={event => { presented.current(event.currentTarget.currentSrc, true); props.onLoad?.(event); }}
    onError={event => { presented.current(event.currentTarget.currentSrc, false); props.onError?.(event); }}/>
}
