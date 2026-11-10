// Réglages : langues, réseau et journal, modèle local, moteur, dossiers,
// sources de skills, à propos. Chaque changement est enregistré aussitôt.
import { useCallback, useEffect, useState } from "react";

import type { Lang } from "../bindings/Lang";
import type { ModelStatus } from "../bindings/ModelStatus";
import type { Settings } from "../bindings/Settings";
import type { SkillSource } from "../bindings/SkillSource";
import { EngineInstall } from "../components/EngineInstall";
import { Banner, Button, Empty, Field, Progress } from "../components/ui";
import { useI18n, type Translate } from "../i18n";
import type { MessageKey } from "../i18n/fr";
import { api } from "../lib/api";
import { formatBytes, formatDate } from "../lib/format";
import type { DownloadProgress, JournalEntry, RuntimeLocation, RuntimeStatus, SystemInfo } from "../lib/types";

type Props = { settings: Settings; onSettings: (s: Settings) => void };

export function SettingsPage({ settings, onSettings }: Props) {
  const { t, err } = useI18n();
  const [error, setError] = useState<string | null>(null);

  const save = useCallback(
    async (next: Settings) => {
      try {
        onSettings(await api.saveSettings(next));
      } catch (e) {
        setError(err(e));
      }
    },
    [onSettings, err],
  );

  return (
    <section className="page settings">
      <header className="page-header">
        <h1>{t("settings.title")}</h1>
      </header>
      {error && (
        <Banner kind="error" onClose={() => setError(null)}>
          {error}
        </Banner>
      )}
      <General settings={settings} save={save} />
      <Models settings={settings} save={save} onError={setError} />
      <Runtime settings={settings} save={save} onError={setError} />
      <Network settings={settings} save={save} onError={setError} />
      <Sources settings={settings} save={save} />
      <Paths settings={settings} save={save} onError={setError} />
      <About />
    </section>
  );
}

type SectionProps = { settings: Settings; save: (s: Settings) => Promise<void>; onError?: (m: string) => void };

function LangSelect({ value, onChange, label }: { value: Lang; onChange: (l: Lang) => void; label: string }) {
  const { t } = useI18n();
  return (
    <Field label={label}>
      <select value={value} onChange={(e) => onChange(e.target.value as Lang)}>
        <option value="fr">{t("lang.fr")}</option>
        <option value="en">{t("lang.en")}</option>
      </select>
    </Field>
  );
}

function General({ settings, save }: SectionProps) {
  const { t } = useI18n();
  return (
    <section className="panel">
      <h2>{t("settings.general")}</h2>
      <div className="grid-2">
        <LangSelect label={t("settings.ui_language")} value={settings.ui_language} onChange={(ui_language) => save({ ...settings, ui_language })} />
        <LangSelect
          label={t("settings.output_language")}
          value={settings.default_output_language}
          onChange={(default_output_language) => save({ ...settings, default_output_language })}
        />
      </div>
    </section>
  );
}

function modelStateText(m: ModelStatus, t: Translate, lang: Lang): string {
  switch (m.state) {
    case "ready":
      return t("settings.models.ready");
    case "partial":
      return t("settings.models.partial", { done: formatBytes(m.bytes_on_disk, lang), total: formatBytes(m.size_bytes, lang) });
    case "not_downloaded":
      return t("settings.models.not_downloaded", { size: formatBytes(m.size_bytes, lang) });
  }
}

function Models({ settings, save, onError }: SectionProps) {
  const { t, lang, err } = useI18n();
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});

  const reload = useCallback(() => {
    api
      .listModels()
      .then(setModels)
      .catch((e: unknown) => onError?.(err(e)));
  }, [err, onError]);

  useEffect(() => {
    api.systemInfo().then(setInfo).catch(() => setInfo(null));
    reload();
  }, [reload]);

  const download = async (id: string) => {
    setProgress((p) => ({ ...p, [id]: { model_id: id, bytes: 0, total: null } }));
    try {
      await api.startModelDownload(id, (event) => setProgress((p) => ({ ...p, [id]: event })));
    } catch (e) {
      onError?.(err(e));
    } finally {
      setProgress((p) => {
        const { [id]: _, ...rest } = p;
        return rest;
      });
      reload();
    }
  };

  const hw = info?.hardware;
  return (
    <section className="panel">
      <h2>{t("settings.models")}</h2>
      {hw && (
        <p className="muted">
          {t("settings.models.hardware", {
            ram: formatBytes(hw.total_memory_bytes, lang),
            threads: hw.cpu_threads,
            os: hw.os,
            arch: hw.arch,
          })}
          {hw.unified_memory && ` · ${t("settings.models.unified")}`}
        </p>
      )}
      {info?.recommendation.below_minimum && <Banner kind="warning">{t("settings.models.below_minimum")}</Banner>}
      <ul className="cards">
        {models.map((m) => {
          const active = settings.active_model === m.id;
          const running = progress[m.id];
          return (
            <li key={m.id} className={`card ${active ? "card-active" : ""}`}>
              <div>
                <strong>{m.label}</strong>{" "}
                {m.role && <span className="chip">{t(`settings.models.role.${m.role}` as MessageKey)}</span>}
                {info?.recommendation.model_id === m.id && <span className="chip chip-accent">{t("settings.models.recommended")}</span>}
                {active && <span className="chip chip-success">{t("settings.models.active")}</span>}
                {m.imported && <span className="chip chip-outline">{t("settings.models.imported")}</span>}
                {m.notes && <p className="small">{m.notes[lang]}</p>}
                <p className="muted small">
                  {modelStateText(m, t, lang)}
                  {m.min_memory_gb !== null && ` · ${t("settings.models.memory", { gb: m.min_memory_gb })}`}
                  {m.license && ` · ${t("settings.models.license", { license: m.license })}`}
                </p>
                {running && (
                  <>
                    <Progress value={running.bytes} max={running.total} label={m.label} />
                    <p className="muted small">
                      {formatBytes(running.bytes, lang)}
                      {running.total !== null && ` / ${formatBytes(running.total, lang)}`}
                    </p>
                  </>
                )}
              </div>
              <div className="inline">
                {m.state === "ready" && !active && (
                  <Button kind="primary" onClick={() => save({ ...settings, active_model: m.id })}>
                    {t("settings.models.use")}
                  </Button>
                )}
                {m.state !== "ready" &&
                  !m.imported &&
                  (running ? (
                    <Button kind="danger" onClick={() => api.cancelModelDownload(m.id)}>
                      {t("settings.models.cancel_download")}
                    </Button>
                  ) : (
                    <Button onClick={() => download(m.id)}>
                      {m.state === "partial" ? t("settings.models.resume") : t("settings.models.download")}
                    </Button>
                  ))}
                {m.imported && (
                  <Button
                    kind="ghost"
                    onClick={async () => {
                      await api.forgetModel(m.id);
                      if (active) await save({ ...settings, active_model: null });
                      reload();
                    }}
                  >
                    {t("settings.models.forget")}
                  </Button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
      <Button
        onClick={async () => {
          try {
            if (await api.importModelFile()) reload();
          } catch (e) {
            onError?.(err(e));
          }
        }}
      >
        {t("settings.models.import")}
      </Button>
    </section>
  );
}

/** Champ numérique facultatif : vide = `null`. Enregistré à la perte du focus. */
function OptionalNumber({ label, value, onCommit }: { label: string; value: number | null; onCommit: (v: number | null) => void }) {
  const [text, setText] = useState(value === null ? "" : String(value));
  useEffect(() => setText(value === null ? "" : String(value)), [value]);
  return (
    <Field label={label}>
      <input
        inputMode="numeric"
        value={text}
        onChange={(e) => setText(e.target.value.replace(/[^0-9]/g, ""))}
        onBlur={() => {
          const next = text === "" ? null : Number(text);
          if (next !== value) onCommit(next);
        }}
      />
    </Field>
  );
}

function Runtime({ settings, save, onError }: SectionProps) {
  const { t, err } = useI18n();
  const [status, setStatus] = useState<RuntimeStatus>({ state: "stopped" });
  const [log, setLog] = useState<string[] | null>(null);
  const [location, setLocation] = useState<RuntimeLocation | null>(null);
  const runtime = settings.runtime;
  const set = (patch: Partial<Settings["runtime"]>) => save({ ...settings, runtime: { ...runtime, ...patch } });

  useEffect(() => {
    const poll = () => api.runtimeStatus().then(setStatus).catch(() => undefined);
    poll();
    const timer = window.setInterval(poll, 2000);
    return () => window.clearInterval(timer);
  }, []);

  const locate = useCallback(() => {
    api.locateRuntime().then(setLocation).catch(() => setLocation(null));
  }, []);
  // Le programme trouvé dépend du chemin choisi dans les réglages.
  useEffect(locate, [locate, runtime.server_path]);

  const showLog = async () => setLog(await api.runtimeLog().catch(() => []));

  return (
    <section className="panel">
      <h2>{t("settings.runtime")}</h2>
      <div className="radio-group" role="radiogroup">
        <label className="check">
          <input type="radio" checked={runtime.backend === "embedded"} onChange={() => set({ backend: "embedded" })} />
          {t("settings.runtime.embedded")}
        </label>
        <label className="check">
          <input type="radio" checked={runtime.backend === "ollama"} onChange={() => set({ backend: "ollama" })} />
          {t("settings.runtime.ollama")}
        </label>
      </div>
      <div className="grid-2">
        {runtime.backend === "ollama" ? (
          <Field label={t("settings.runtime.ollama_model")}>
            <input
              defaultValue={runtime.ollama_model ?? ""}
              placeholder="qwen3:8b"
              onBlur={(e) => set({ ollama_model: e.target.value.trim() || null })}
            />
          </Field>
        ) : (
          <>
            <OptionalNumber label={t("settings.runtime.gpu_layers")} value={runtime.gpu_layers} onCommit={(gpu_layers) => set({ gpu_layers })} />
            <OptionalNumber
              label={t("settings.runtime.cpu_moe")}
              value={runtime.cpu_moe_layers}
              onCommit={(cpu_moe_layers) => set({ cpu_moe_layers })}
            />
          </>
        )}
        <OptionalNumber
          label={t("settings.runtime.context")}
          value={runtime.context_size}
          onCommit={(v) => set({ context_size: Math.max(4096, v ?? 32768) })}
        />
      </div>
      {runtime.backend === "embedded" && location && (
        <div className="engine-location">
          <strong>{t("settings.runtime.program")}</strong>{" "}
          {location.found ? (
            <>
              <code>{location.found.path}</code>{" "}
              <span className="chip">{t(`settings.runtime.source.${location.found.source}` as MessageKey)}</span>
            </>
          ) : (
            <span className="chip chip-warning">{t("settings.runtime.program.missing")}</span>
          )}{" "}
          <Button
            kind="ghost"
            onClick={async () => {
              try {
                const picked = await api.pickLlamaServer();
                if (picked) await set({ server_path: picked });
              } catch (e) {
                onError?.(err(e));
              }
            }}
          >
            {t("settings.runtime.program.pick")}
          </Button>
          {runtime.server_path && (
            <Button kind="ghost" onClick={() => set({ server_path: null })}>
              {t("settings.runtime.program.auto")}
            </Button>
          )}
          {!location.found && (
            <Banner kind="warning">
              <p>{t("settings.runtime.program.help")}</p>
              {location.install_target && !runtime.server_path && <EngineInstall onInstalled={locate} />}
              <details>
                <summary>{t("settings.runtime.program.searched")}</summary>
                <ul className="files">
                  {location.searched.map((path) => (
                    <li key={path}>
                      <code>{path}</code>
                    </li>
                  ))}
                </ul>
              </details>
            </Banner>
          )}
        </div>
      )}
      <p>
        <RuntimeBadge status={status} />
      </p>
      {/* Moteur absent : l'encadré ci-dessus l'explique déjà et propose l'installation. */}
      {status.state === "failed" && !(status.code === "runtime.missing" && location && !location.found) && (
        <Banner kind="error">
          <p className="pre-wrap">{err({ code: status.code, message: status.message })}</p>
        </Banner>
      )}
      <div className="inline">
        <Button
          kind="primary"
          disabled={status.state === "starting"}
          onClick={async () => {
            try {
              setStatus({ state: "starting", model: settings.active_model ?? runtime.ollama_model ?? "" });
              setStatus(await api.startRuntime());
            } catch {
              // L'erreur est affichée sous l'état, et le journal dit ce qui a été tenté.
              setStatus(await api.runtimeStatus());
              await showLog();
            }
            locate();
          }}
        >
          {t("settings.runtime.start")}
        </Button>
        <Button onClick={async () => setStatus(await api.stopRuntime())}>{t("settings.runtime.stop")}</Button>
        <Button kind="ghost" onClick={showLog}>
          {t("settings.runtime.log")}
        </Button>
      </div>
      {log && (log.length === 0 ? <Empty>{t("settings.runtime.log.empty")}</Empty> : <pre className="log">{log.join("\n")}</pre>)}
    </section>
  );
}

export function RuntimeBadge({ status }: { status: RuntimeStatus }) {
  const { t } = useI18n();
  switch (status.state) {
    case "stopped":
      return <span className="pill pill-muted">{t("runtime.stopped")}</span>;
    case "starting":
      return <span className="pill pill-warning">{t("runtime.starting", { model: status.model })}</span>;
    case "ready":
      return <span className="pill pill-success">{t("runtime.ready", { model: status.model })}</span>;
    case "failed":
      return (
        <span className="pill pill-error">{t("runtime.failed")}</span>
      );
  }
}

export function journalSummary(entry: JournalEntry, t: Translate): string {
  if (entry.event === "decision") {
    const decision = entry.decision ? t(`journal.decision.${entry.decision}` as MessageKey) : "";
    return [decision, entry.purpose].filter(Boolean).join(" · ");
  }
  if (entry.event === "blocked") return `${t("journal.blocked")} · ${entry.url ?? entry.hosts?.join(", ") ?? ""}`;
  const status = entry.status ?? entry.error ?? "";
  return `${entry.method ?? "GET"} ${entry.url ?? ""} → ${status}`;
}

function Network({ settings, save, onError }: SectionProps) {
  const { t, lang, err } = useI18n();
  const [journal, setJournal] = useState<JournalEntry[] | null>(null);
  const load = useCallback(() => {
    api
      .readNetworkLog(200)
      .then(setJournal)
      .catch((e: unknown) => onError?.(err(e)));
  }, [err, onError]);
  useEffect(load, [load]);

  return (
    <section className="panel">
      <h2>{t("settings.network")}</h2>
      <div className="radio-group" role="radiogroup">
        <label className="check">
          <input type="radio" checked={settings.network_policy === "ask"} onChange={() => save({ ...settings, network_policy: "ask" })} />
          {t("settings.network.ask")}
        </label>
        <label className="check">
          <input type="radio" checked={settings.network_policy === "off"} onChange={() => save({ ...settings, network_policy: "off" })} />
          {t("settings.network.off")}
        </label>
      </div>
      <p className="field-hint">{t("settings.network.hint")}</p>
      <header className="panel-header">
        <h3>{t("settings.journal")}</h3>
        <Button kind="ghost" onClick={load}>
          {t("settings.journal.refresh")}
        </Button>
      </header>
      {journal && journal.length === 0 && <Empty>{t("settings.journal.empty")}</Empty>}
      {journal && journal.length > 0 && (
        <table className="table journal">
          <thead>
            <tr>
              <th>{t("journal.col.date")}</th>
              <th>{t("journal.col.action")}</th>
              <th>{t("journal.col.detail")}</th>
            </tr>
          </thead>
          <tbody>
            {journal.map((entry, i) => (
              <tr key={`${entry.action_id}-${i}`} className={`journal-${entry.event}`}>
                <td>{formatDate(entry.ts, lang)}</td>
                <td>{t(`journal.action.${entry.action}` as MessageKey)}</td>
                <td className="wrap">{journalSummary(entry, t)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

function newSourceId(): string {
  return `src-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

function Sources({ settings, save }: SectionProps) {
  const { t } = useI18n();
  const sources = settings.skill_sources;
  const [adding, setAdding] = useState<"github" | "registry" | null>(null);
  const [form, setForm] = useState({ label: "", owner: "", repo: "", branch: "main", prefix: "", url: "" });
  const setSources = (next: SkillSource[]) => save({ ...settings, skill_sources: next });

  const add = () => {
    const label = form.label.trim();
    let source: SkillSource | null = null;
    if (adding === "github" && form.owner.trim() && form.repo.trim()) {
      source = {
        id: newSourceId(),
        label: label || `${form.owner.trim()}/${form.repo.trim()}`,
        enabled: true,
        kind: {
          type: "github_repo",
          owner: form.owner.trim(),
          repo: form.repo.trim(),
          branch: form.branch.trim() || "main",
          path_prefix: form.prefix.trim() || null,
        },
      };
    } else if (adding === "registry" && form.url.trim().startsWith("https://")) {
      source = { id: newSourceId(), label: label || form.url.trim(), enabled: true, kind: { type: "registry", index_url: form.url.trim() } };
    }
    if (!source) return;
    void setSources([...sources, source]);
    setAdding(null);
    setForm({ label: "", owner: "", repo: "", branch: "main", prefix: "", url: "" });
  };

  return (
    <section className="panel">
      <h2>{t("settings.sources")}</h2>
      <ul className="cards">
        {sources.map((s) => (
          <li key={s.id} className="card">
            <label className="check">
              <input
                type="checkbox"
                checked={s.enabled}
                onChange={(e) => setSources(sources.map((x) => (x.id === s.id ? { ...x, enabled: e.target.checked } : x)))}
              />
              <span>
                <strong>{s.label}</strong>
                <span className="muted small">
                  {" "}
                  {s.kind.type === "github_repo"
                    ? `github.com/${s.kind.owner}/${s.kind.repo}@${s.kind.branch}${s.kind.path_prefix ? `/${s.kind.path_prefix}` : ""}`
                    : s.kind.index_url}
                </span>
              </span>
            </label>
            <Button kind="ghost" onClick={() => setSources(sources.filter((x) => x.id !== s.id))}>
              {t("common.delete")}
            </Button>
          </li>
        ))}
      </ul>
      {adding === null ? (
        <div className="inline">
          <Button onClick={() => setAdding("github")}>{t("settings.sources.add_github")}</Button>
          <Button onClick={() => setAdding("registry")}>{t("settings.sources.add_registry")}</Button>
        </div>
      ) : (
        <div className="subpanel">
          <Field label={t("settings.sources.label")}>
            <input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} />
          </Field>
          {adding === "github" ? (
            <div className="grid-2">
              <Field label={t("settings.sources.owner")}>
                <input value={form.owner} onChange={(e) => setForm({ ...form, owner: e.target.value })} />
              </Field>
              <Field label={t("settings.sources.repo")}>
                <input value={form.repo} onChange={(e) => setForm({ ...form, repo: e.target.value })} />
              </Field>
              <Field label={t("settings.sources.branch")}>
                <input value={form.branch} onChange={(e) => setForm({ ...form, branch: e.target.value })} />
              </Field>
              <Field label={t("settings.sources.prefix")}>
                <input value={form.prefix} placeholder="skills" onChange={(e) => setForm({ ...form, prefix: e.target.value })} />
              </Field>
            </div>
          ) : (
            <Field label={t("settings.sources.url")}>
              <input value={form.url} placeholder="https://" onChange={(e) => setForm({ ...form, url: e.target.value })} />
            </Field>
          )}
          <div className="form-actions">
            <Button kind="primary" onClick={add}>
              {t("settings.sources.add")}
            </Button>
            <Button onClick={() => setAdding(null)}>{t("common.cancel")}</Button>
          </div>
        </div>
      )}
    </section>
  );
}

function Paths({ settings, save, onError }: SectionProps) {
  const { t, err } = useI18n();
  const [info, setInfo] = useState<SystemInfo | null>(null);
  useEffect(() => {
    api.systemInfo().then(setInfo).catch(() => setInfo(null));
  }, [settings.models_dir]);

  const pick = async (apply: (dir: string) => Settings) => {
    try {
      const dir = await api.pickFolder();
      if (dir) await save(apply(dir));
    } catch (e) {
      onError?.(err(e));
    }
  };

  return (
    <section className="panel">
      <h2>{t("settings.paths")}</h2>
      <dl className="paths">
        <dt>{t("settings.paths.data")}</dt>
        <dd>
          <code>{info?.data_dir ?? "…"}</code>{" "}
          <Button kind="ghost" onClick={() => api.revealDataDir()}>
            {t("common.open_folder")}
          </Button>
        </dd>
        <dt>{t("settings.paths.models")}</dt>
        <dd>
          <code>{info?.models_dir ?? "…"}</code>{" "}
          <Button kind="ghost" onClick={() => pick((models_dir) => ({ ...settings, models_dir }))}>
            {t("settings.paths.models_custom")}
          </Button>
          {settings.models_dir && (
            <Button kind="ghost" onClick={() => save({ ...settings, models_dir: null })}>
              {t("settings.paths.models_reset")}
            </Button>
          )}
        </dd>
        <dt>{t("settings.paths.export")}</dt>
        <dd>
          <code>{settings.default_export_dir ?? t("common.none")}</code>{" "}
          <Button kind="ghost" onClick={() => pick((default_export_dir) => ({ ...settings, default_export_dir }))}>
            {t("settings.paths.models_custom")}
          </Button>
          {settings.default_export_dir && (
            <Button kind="ghost" onClick={() => save({ ...settings, default_export_dir: null })}>
              {t("settings.paths.models_reset")}
            </Button>
          )}
        </dd>
      </dl>
    </section>
  );
}

function About() {
  const { t } = useI18n();
  const [version, setVersion] = useState("");
  useEffect(() => {
    api
      .systemInfo()
      .then((i) => setVersion(i.version))
      .catch(() => setVersion(""));
  }, []);
  return (
    <section className="panel">
      <h2>{t("settings.about")}</h2>
      <p>{t("settings.about.version", { version })}</p>
      <p className="muted">{t("app.offline.hint")}</p>
      <p className="muted">{t("settings.about.privacy")}</p>
    </section>
  );
}
