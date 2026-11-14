"use client";

import { useEffect, useRef } from "react";
import { mountLoadingOrb } from "../../../../client/ui/loading-orb.js";

type Props = {
  size?: number;
  label?: string;
};

export function LoadingOrb({ size = 16, label }: Props) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    if (!canvas.current) return;
    const orb = mountLoadingOrb(canvas.current, size <= 28);
    orb.setRunning(true);
    return orb.dispose;
  }, [size]);

  return (
    <canvas
      ref={canvas}
      data-loading-orb
      role={label ? "status" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      style={{ display: "block", flex: "none", width: size, height: size }}
    />
  );
}
