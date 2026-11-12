"use client";

import { Download } from "lucide-react";
import { useEffect, useState } from "react";
import { FloatingWindow, WindowTrigger } from "./floating-window";
import { useI18n } from "./locale-provider";
import { Button } from "./ui/button";
import { LoadingOrb } from "./ui/loading-orb";

type Update = { version: string; name: string; notes: string };
type Native = { invoke<T>(command: string, args?: Record<string, string>): Promise<T> };

function nativeUpdater() {
  if (
    location.protocol !== "http:" ||
    location.port !== "17860" ||
    !["127.0.0.1", "localhost", "[::1]"].includes(location.hostname)
  ) return;
  return (window as Window & { __TAURI_INTERNALS__?: Native }).__TAURI_INTERNALS__;
}

export function ClientUpdate({ open, onOpen, onClose }: {
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [offer, setOffer] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    const updater = nativeUpdater();
    if (!updater) return;
    let live = true;
    const refresh = () => {
      void updater.invoke<Update | null>("check_client_update").then(update => {
        if (live) setOffer(update);
      }, () => {});
    };
    refresh();
    const interval = setInterval(refresh, 30 * 60 * 1000);
    return () => {
      live = false;
      clearInterval(interval);
    };
  }, []);

  useEffect(() => {
    if (open && !offer) onClose();
  }, [open, offer, onClose]);

  async function install(version: string) {
    const updater = nativeUpdater();
    if (!updater || installing) return;
    setInstalling(true);
    setError("");
    try {
      await updater.invoke("install_client_update", { version });
    } catch (failure) {
      setError(typeof failure === "string" && failure ? failure : t("更新中断，请重试"));
      setInstalling(false);
    }
  }

  if (!offer) return null;
  const label = t("系统更新");
  return <>
    <span className="client-update-trigger">
      <WindowTrigger label={label} open={open} onOpen={onOpen} openOnFocus>
        <Download size={18} aria-hidden />
      </WindowTrigger>
    </span>
    <FloatingWindow id="client-update" title={label} open={open} onClose={onClose}
      width={360} height={400} fitContent focusPanel>
      <div className="client-update-details" aria-busy={installing}>
        <p className="client-update-release" dir="auto">
          <span>{offer.version}</span> <strong>{offer.name}</strong>
        </p>
        <p className="client-update-notes" dir="auto">{offer.notes}</p>
        {error && <p className="client-update-error" role="alert">{error}</p>}
        <Button type="button" disabled={installing} onClick={() => void install(offer.version)}>
          {installing && <LoadingOrb />}
          {t(installing ? "正在更新…" : "确认更新")}
        </Button>
      </div>
    </FloatingWindow>
  </>;
}
