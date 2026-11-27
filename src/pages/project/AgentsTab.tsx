// AGENTS.md : génération complète ou par section, édition, aperçu, contrôles.
import { useEffect, useMemo, useState } from "react";

import type { GenEvent } from "../../bindings/GenEvent";
import type { Issue } from "../../bindings/Issue";
import type { SectionId } from "../../bindings/SectionId";
import { EngineInstall } from "../../components/EngineInstall";
import { IssuesList } from "../../components/IssuesList";
import { MarkdownEditor } from "../../components/MarkdownEditor";
import { MarkdownPreview } from "../../components/MarkdownPreview";
import { StreamView } from "../../components/StreamView";
import { Banner, Button, Empty, Progress } from "../../components/ui";
import { useI18n } from "../../i18n";
import type { MessageKey } from "../../i18n/fr";
import { api, isEngineMissing } from "../../lib/api";
import { stripMarkers } from "../../lib/format";
import type { ProjectDetail } from "../../lib/types";

export const SECTIONS: SectionId[] = [
  "context",
  "goals",
  "stack",
  "structure",
  "commands",
  "conventions",
  "quality",
  "prohibitions",
  "git",
  "ui",
];

type View = "edit" | "preview" | "split";
type Run = { index: number; total: number; section: SectionId | null; text: string; reasoning: string; retrying: boolean };

/** Numéro de la dernière version enregistrée, ou `null` s'il n'y en a aucune. */
const latestVersion = (versions: ProjectDetail["versions"]) =>
  versions.reduce<number | null>((max, v) => (max === null || v.number > max ? v.number : max), null);

export function AgentsTab({ detail, onChanged }: { detail: ProjectDetail; onChanged: () => void }) {
  const { t, err } = useI18n();
  const id = detail.project.id;
  const [content, setContent] = useState(detail.agents ?? "");
  const [saved, setSaved] = useState(detail.agents ?? "");
  const [version, setVersion] = useState(() => latestVersion(detail.versions));
  const [view, setView] = useState<View>("split");
  const [run, setRun] = useState<Run | null>(null);
  const [issues, setIssues] = useState<Issue[] | null>(null);
  const [section, setSection] = useState<SectionId>("commands");
  const [instruction, setInstruction] = useState("");
  const [note, setNote] = useState("");
  const [message, setMessage] = useState<{ kind: "error" | "success" | "info"; text: string; engineMissing?: boolean } | null>(null);
  const dirty = content !== saved;

  useEffect(() => {
    setContent(detail.agents ?? "");
    setSaved(detail.agents ?? "");
  }, [detail.agents]);

  useEffect(() => {
    setVersion(latestVersion(detail.versions));
  }, [detail.versions]);

  const onEvent = (e: GenEvent) =>
    setRun((prev) => {
      const r = prev ?? { index: 0, total: 0, section: null, text: "", reasoning: "", retrying: false };
      switch (e.type) {
        case "preparing":
          return r;
        case "section_started":
          return { index: e.index + 1, total: e.total, section: e.section, text: "", reasoning: "", retrying: false };
        case "token":
          return { ...r, text: r.text + e.text };
        case "reasoning":
          return { ...r, reasoning: r.reasoning + e.text };
        case "retrying":
          return { ...r, text: "", reasoning: "", retrying: true };
        case "section_done":
          return r;
      }
    });

  const generate = async (sections: SectionId[] | null) => {
    setMessage(null);
    setRun({ index: 0, total: sections?.length ?? SECTIONS.length, section: null, text: "", reasoning: "", retrying: false });
    try {
      const result = await api.generateAgents(id, { sections, instruction: sections ? instruction.trim() || null : null }, onEvent);
      setContent(result.content);
      setSaved(result.content);
      setVersion(result.version.number);
      setIssues(result.issues);
      setInstruction("");
      setMessage({ kind: "success", text: t("agents.done", { number: result.version.number }) });
      onChanged();
    } catch (e) {
      setMessage({ kind: "error", text: err(e), engineMissing: isEngineMissing(e) });
    } finally {
      setRun(null);
    }
  };

  const save = async () => {
    try {
      const meta = await api.saveAgents(id, content, note.trim() || null);
      setSaved(content);
      setVersion(meta.number);
      setNote("");
      setMessage({ kind: "success", text: t("agents.done", { number: meta.number }) });
      setIssues(await api.checkAgents(id));
      onChanged();
    } catch (e) {
      setMessage({ kind: "error", text: err(e) });
    }
  };

  const check = async () => {
    try {
      setIssues(await api.checkAgents(id));
    } catch (e) {
      setMessage({ kind: "error", text: err(e) });
    }
  };

  const preview = useMemo(() => stripMarkers(content), [content]);
  const busy = run !== null;
  // Rien à enregistrer : on dit pourquoi le bouton est grisé et comment l'activer.
  const upToDate = !dirty && !busy && version !== null;
  const upToDateHint = upToDate ? t("agents.up_to_date.hint", { number: version }) : undefined;

  return (
    <div className="agents">
      {message && (
        <Banner kind={message.kind} onClose={() => setMessage(null)}>
          <p>{message.text}</p>
          {message.engineMissing && <EngineInstall onInstalled={() => setMessage(null)} />}
        </Banner>
      )}
      <div className="toolbar">
        <Button kind="primary" disabled={busy || dirty} onClick={() => generate(null)} title={dirty ? t("agents.unsaved") : undefined}>
          {saved ? t("agents.regenerate_all") : t("agents.generate")}
        </Button>
        {saved && (
          <>
            <select aria-label={t("agents.sections")} value={section} onChange={(e) => setSection(e.target.value as SectionId)} disabled={busy}>
              {SECTIONS.map((s) => (
                <option key={s} value={s}>
                  {t(`section.${s}` as MessageKey)}
                </option>
              ))}
            </select>
            <input
              className="grow"
              aria-label={t("agents.instruction")}
              placeholder={t("agents.instruction.placeholder")}
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
              disabled={busy}
            />
            <Button disabled={busy || dirty} onClick={() => generate([section])}>
              {t("agents.regenerate_section")}
            </Button>
          </>
        )}
        {busy && (
          <Button kind="danger" onClick={() => api.cancelOperation(`generate:${id}`)}>
            {t("agents.cancel")}
          </Button>
        )}
      </div>

      {run && (
        <div className="run">
          <Progress value={run.index} max={run.total || null} />
          <p className="muted">
            {run.section
              ? t("agents.progress", { index: run.index, total: run.total, section: t(`section.${run.section}` as MessageKey) })
              : t("agents.preparing")}
          </p>
          {run.retrying && <p className="muted">{t("agents.retrying")}</p>}
          <StreamView text={run.text} reasoning={run.reasoning} />
        </div>
      )}

      {!saved && !busy ? (
        <Empty>{t("agents.empty")}</Empty>
      ) : (
        <>
          <div className="toolbar">
            <div className="segmented" role="group">
              {(["edit", "split", "preview"] as View[]).map((v) => (
                <button key={v} type="button" aria-pressed={view === v} className={view === v ? "on" : ""} onClick={() => setView(v)}>
                  {t(`agents.${v}` as MessageKey)}
                </button>
              ))}
            </div>
            {dirty && <span className="chip chip-warning">{t("agents.unsaved")}</span>}
            {upToDate && (
              <span className="chip chip-success" title={upToDateHint}>
                {t("agents.up_to_date", { number: version })}
              </span>
            )}
            <input
              className="grow"
              aria-label={t("agents.note")}
              placeholder={t("agents.note")}
              value={note}
              onChange={(e) => setNote(e.target.value)}
              disabled={!dirty || busy}
              title={upToDateHint}
            />
            <Button kind="primary" disabled={!dirty || busy} onClick={save} title={upToDateHint}>
              {t("agents.save")}
            </Button>
            <Button onClick={check} disabled={busy}>
              {t("agents.check")}
            </Button>
          </div>
          <div className={`editor-area view-${view}`}>
            {view !== "preview" && <MarkdownEditor label="AGENTS.md" value={content} onChange={setContent} readOnly={busy} />}
            {view !== "edit" && <MarkdownPreview source={preview} onError={(e) => setMessage({ kind: "error", text: err(e) })} />}
          </div>
          {issues && (
            <section className="panel">
              <h3>{t("agents.issues")}</h3>
              <IssuesList issues={issues} />
            </section>
          )}
        </>
      )}
    </div>
  );
}
