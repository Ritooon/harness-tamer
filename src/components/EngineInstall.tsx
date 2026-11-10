// Installation du moteur llama.cpp en un clic, sans console : le cœur ouvre
// le dialogue de consentement, télécharge l'archive officielle, vérifie son
// empreinte SHA-256 et l'installe dans les données de l'application.
import { useState } from "react";

import { useI18n } from "../i18n";
import { api } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { DownloadProgress } from "../lib/types";
import { Banner, Button, Progress } from "./ui";

export function EngineInstall({ onInstalled }: { onInstalled?: () => void }) {
  const { t, lang, err } = useI18n();
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const install = async () => {
    setError(null);
    setProgress({ model_id: "llama.cpp", bytes: 0, total: null });
    try {
      const installed = await api.installLlamaRuntime(setProgress);
      setDone(installed.tag);
      onInstalled?.();
    } catch (e) {
      setError(err(e));
    } finally {
      setProgress(null);
    }
  };

  if (done) return <Banner kind="success">{t("engine.installed", { tag: done })}</Banner>;
  return (
    <div className="engine-install">
      {error && <Banner kind="error">{error}</Banner>}
      {progress ? (
        <div>
          <Progress value={progress.bytes} max={progress.total} label={t("engine.install")} />
          <p className="muted small">
            {t("engine.installing")} {formatBytes(progress.bytes, lang)}
            {progress.total !== null && ` / ${formatBytes(progress.total, lang)}`}
          </p>
        </div>
      ) : (
        <>
          <Button kind="primary" onClick={install}>
            {t("engine.install")}
          </Button>
          <span className="field-hint">{t("engine.install.hint")}</span>
        </>
      )}
    </div>
  );
}
