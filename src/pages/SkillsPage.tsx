// Bibliothèque de skills : création (vide ou assistée par le modèle local),
// recherche et import en ligne sous consentement, édition, suppression.
import { useCallback, useEffect, useState } from "react";

import type { OnlineSkill } from "../bindings/OnlineSkill";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import type { SkillFile } from "../bindings/SkillFile";
import type { SkillMeta } from "../bindings/SkillMeta";
import type { SkillOrigin } from "../bindings/SkillOrigin";
import { MarkdownPreview } from "../components/MarkdownPreview";
import { SkillEditor } from "../components/SkillEditor";
import { StreamView } from "../components/StreamView";
import { Banner, Button, Empty, Field, Modal } from "../components/ui";
import { useI18n, type Translate } from "../i18n";
import { api } from "../lib/api";
import { formatDate } from "../lib/format";
import { navigate } from "../lib/router";
import type { TextEvent } from "../lib/types";

/** Brouillon d'un nouveau skill, avant son entrée dans la bibliothèque. */
type NewDraft = { content: string; files: SkillFile[]; origin: SkillOrigin; notes: string[] };

export function originText(origin: SkillOrigin, t: Translate): string {
  switch (origin.type) {
    case "created":
      return t("skills.origin.created");
    case "imported":
      return t("skills.origin.imported");
    case "adapted":
      return t("skills.origin.adapted");
  }
}

export function SkillsPage({ slug }: { slug?: string }) {
  const { t, err } = useI18n();
  const [skills, setSkills] = useState<SkillMeta[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"blank" | "assisted" | "online" | null>(null);
  const [draft, setDraft] = useState<NewDraft | null>(null);

  const reload = useCallback(() => {
    api
      .listSkills()
      .then(setSkills)
      .catch((e: unknown) => setError(err(e)));
  }, [err]);

  useEffect(reload, [reload]);

  const saveDraft = async () => {
    if (!draft) return;
    try {
      const skill = await api.saveNewSkill(draft.content, draft.files, draft.origin);
      setDraft(null);
      reload();
      navigate({ name: "skills", slug: skill.meta.slug });
    } catch (e) {
      setError(err(e));
    }
  };

  return (
    <section className="page">
      <header className="page-header">
        <h1>{t("skills.title")}</h1>
        <div className="inline">
          <Button onClick={() => setMode("blank")}>{t("skills.new_blank")}</Button>
          <Button kind="primary" onClick={() => setMode("assisted")}>
            {t("skills.new_assisted")}
          </Button>
          <Button onClick={() => setMode("online")}>{t("skills.online")}</Button>
        </div>
      </header>
      {error && (
        <Banner kind="error" onClose={() => setError(null)}>
          {error}
        </Banner>
      )}
      <div className="master-detail">
        <nav className="list" aria-label={t("skills.title")}>
          {skills === null ? (
            <p className="muted">{t("common.loading")}</p>
          ) : skills.length === 0 ? (
            <Empty>{t("skills.empty")}</Empty>
          ) : (
            <ul>
              {skills.map((s) => (
                <li key={s.slug}>
                  <a href={`#/skills/${encodeURIComponent(s.slug)}`} className={s.slug === slug ? "active" : ""}>
                    <strong>{s.name}</strong>
                    <span className="muted small">
                      {originText(s.origin, t)} · {t("skills.version", { version: s.version })}
                    </span>
                  </a>
                </li>
              ))}
            </ul>
          )}
        </nav>
        <div className="detail">
          {slug ? (
            <LibrarySkill key={slug} slug={slug} onChanged={reload} onError={setError} />
          ) : (
            <Empty>{t("skills.select")}</Empty>
          )}
        </div>
      </div>

      {mode === "blank" && <BlankSkill onClose={() => setMode(null)} onDraft={(d) => (setMode(null), setDraft(d))} />}
      {mode === "assisted" && <AssistedSkill onClose={() => setMode(null)} onDraft={(d) => (setMode(null), setDraft(d))} />}
      {mode === "online" && <OnlineSearch onClose={() => setMode(null)} onDraft={(d) => (setMode(null), setDraft(d))} />}

      {draft && (
        <Modal title={t("skills.save_to_library")} onClose={() => setDraft(null)} wide>
          {draft.notes.length > 0 && (
            <Banner kind="warning">
              <ul>
                {draft.notes.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            </Banner>
          )}
          <SkillEditor content={draft.content} files={draft.files} onChange={(content, files) => setDraft({ ...draft, content, files })} />
          <div className="form-actions">
            <Button kind="primary" onClick={saveDraft}>
              {t("skills.save_to_library")}
            </Button>
            <Button onClick={() => setDraft(null)}>{t("common.cancel")}</Button>
          </div>
        </Modal>
      )}
    </section>
  );
}

/** Skill de la bibliothèque : édition et suppression. */
function LibrarySkill({ slug, onChanged, onError }: { slug: string; onChanged: () => void; onError: (m: string) => void }) {
  const { t, lang, err } = useI18n();
  const [meta, setMeta] = useState<SkillMeta | null>(null);
  const [content, setContent] = useState("");
  const [files, setFiles] = useState<SkillFile[]>([]);
  const [dirty, setDirty] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api
      .getSkill(slug)
      .then((s) => {
        setMeta(s.meta);
        setContent(s.content);
        setFiles(s.files);
      })
      .catch((e: unknown) => onError(err(e)));
  }, [slug, err, onError]);

  if (!meta) return <p className="muted">{t("common.loading")}</p>;

  const save = async () => {
    try {
      const s = await api.updateSkill(slug, content, files);
      setMeta(s.meta);
      setDirty(false);
      setSaved(true);
      onChanged();
    } catch (e) {
      onError(err(e));
    }
  };

  return (
    <div>
      <header className="panel-header">
        <div>
          <h2>{meta.name}</h2>
          <p className="muted small">
            {originText(meta.origin, t)}
            {meta.origin.type === "imported" && ` · ${meta.origin.url}`} · {t("skills.version", { version: meta.version })} ·{" "}
            {formatDate(meta.updated_at, lang)}
          </p>
        </div>
        <div className="inline">
          <Button kind="primary" onClick={save} disabled={!dirty}>
            {t("common.save")}
          </Button>
          <Button
            kind="danger"
            onClick={async () => {
              try {
                if (await api.deleteSkill(slug)) {
                  onChanged();
                  navigate({ name: "skills" });
                }
              } catch (e) {
                onError(err(e));
              }
            }}
          >
            {t("common.delete")}
          </Button>
        </div>
      </header>
      {saved && !dirty && (
        <Banner kind="success" onClose={() => setSaved(false)}>
          {t("common.saved")}
        </Banner>
      )}
      <SkillEditor
        content={content}
        files={files}
        onChange={(c, f) => {
          setContent(c);
          setFiles(f);
          setDirty(true);
        }}
      />
    </div>
  );
}

type DraftProps = { onClose: () => void; onDraft: (d: NewDraft) => void };

function BlankSkill({ onClose, onDraft }: DraftProps) {
  const { t, err } = useI18n();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const create = async () => {
    try {
      const content = await api.blankSkillTemplate(name.trim(), description.trim());
      onDraft({ content, files: [], origin: { type: "created" }, notes: [] });
    } catch (e) {
      setError(err(e));
    }
  };
  return (
    <Modal title={t("skills.new_blank")} onClose={onClose}>
      {error && <Banner kind="error">{error}</Banner>}
      <Field label={t("skills.blank.name")}>
        <input value={name} placeholder="review-react-pr" onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label={t("skills.blank.description")}>
        <textarea rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
      </Field>
      <div className="form-actions">
        <Button kind="primary" onClick={create} disabled={!name.trim()}>
          {t("common.edit")}
        </Button>
      </div>
    </Modal>
  );
}

function AssistedSkill({ onClose, onDraft }: DraftProps) {
  const { t, err } = useI18n();
  const [request, setRequest] = useState("");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectId, setProjectId] = useState("");
  const [running, setRunning] = useState(false);
  const [text, setText] = useState("");
  const [reasoning, setReasoning] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listProjects({ text: null, status: "active", sort: "name_asc" })
      .then(setProjects)
      .catch(() => setProjects([]));
  }, []);

  const propose = async () => {
    setRunning(true);
    setText("");
    setReasoning("");
    setError(null);
    const onEvent = (e: TextEvent) => (e.type === "token" ? setText((v) => v + e.text) : setReasoning((v) => v + e.text));
    try {
      const p = await api.proposeNewSkill(request.trim(), projectId || null, onEvent);
      onDraft({ content: p.content, files: p.files, origin: { type: "created" }, notes: p.warnings });
    } catch (e) {
      setError(err(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <Modal title={t("skills.new_assisted")} onClose={() => (running ? undefined : onClose())} wide>
      {error && <Banner kind="error">{error}</Banner>}
      <Field label={t("skills.request")}>
        <textarea rows={4} value={request} disabled={running} placeholder={t("skills.request.placeholder")} onChange={(e) => setRequest(e.target.value)} />
      </Field>
      <Field label={t("skills.project_context")}>
        <select value={projectId} disabled={running} onChange={(e) => setProjectId(e.target.value)}>
          <option value="">{t("skills.no_project")}</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </Field>
      <div className="form-actions">
        {running ? (
          <Button kind="danger" onClick={() => api.cancelOperation("skill:propose")}>
            {t("agents.cancel")}
          </Button>
        ) : (
          <Button kind="primary" onClick={propose} disabled={!request.trim()}>
            {t("skills.propose")}
          </Button>
        )}
      </div>
      {(running || text) && <StreamView text={text} reasoning={reasoning} />}
    </Modal>
  );
}

function OnlineSearch({ onClose, onDraft }: DraftProps) {
  const { t, err } = useI18n();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<OnlineSkill[] | null>(null);
  const [selected, setSelected] = useState<OnlineSkill | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const search = async () => {
    setBusy(true);
    setError(null);
    try {
      setResults(await api.searchOnlineSkills(query.trim()));
      setSelected(null);
    } catch (e) {
      setError(err(e));
    } finally {
      setBusy(false);
    }
  };

  const importSkill = async (found: OnlineSkill) => {
    setBusy(true);
    setError(null);
    try {
      const imported = await api.importOnlineSkill(found);
      const notes = imported.skipped.length ? [t("skills.online.skipped", { files: imported.skipped.join(", ") })] : [];
      onDraft({ content: imported.content, files: imported.files, origin: imported.origin, notes });
    } catch (e) {
      setError(err(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t("skills.online")} onClose={onClose} wide>
      <Banner kind="info">{t("skills.online.hint")}</Banner>
      {error && <Banner kind="error">{error}</Banner>}
      <form
        className="inline"
        onSubmit={(e) => {
          e.preventDefault();
          void search();
        }}
      >
        <Field label={t("skills.online.query")}>
          <input value={query} onChange={(e) => setQuery(e.target.value)} />
        </Field>
        <Button type="submit" kind="primary" disabled={busy}>
          {t("skills.online.search")}
        </Button>
      </form>
      {results && results.length === 0 && <Empty>{t("skills.online.none")}</Empty>}
      {results && results.length > 0 && (
        <div className="master-detail">
          <ul className="list">
            {results.map((r) => (
              <li key={r.url}>
                <button type="button" className={`list-item ${selected?.url === r.url ? "active" : ""}`} onClick={() => setSelected(r)}>
                  <strong>{r.name}</strong>
                  <span className="muted small">{r.source_label}</span>
                </button>
              </li>
            ))}
          </ul>
          <div className="detail">
            {selected ? (
              <>
                <p className="muted small">{selected.url}</p>
                <p>{selected.description}</p>
                {selected.resources.length > 0 && (
                  <p className="small">
                    {t("skills.files")} : {selected.resources.join(", ")}
                  </p>
                )}
                <div className="form-actions">
                  <Button kind="primary" disabled={busy} onClick={() => importSkill(selected)}>
                    {t("skills.online.import")}
                  </Button>
                </div>
                <div className="panel">
                  <MarkdownPreview source={selected.preview.replace(/^---[\s\S]*?\n---\n/, "")} />
                </div>
              </>
            ) : (
              <Empty>{t("skills.select")}</Empty>
            )}
          </div>
        </div>
      )}
    </Modal>
  );
}
