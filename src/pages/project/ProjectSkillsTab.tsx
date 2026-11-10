// Skills d'un projet : copies rattachées depuis la bibliothèque, éditables
// et adaptables au contexte du projet par le modèle local.
import { useCallback, useEffect, useState } from "react";

import type { Skill } from "../../bindings/Skill";
import type { SkillFile } from "../../bindings/SkillFile";
import type { SkillMeta } from "../../bindings/SkillMeta";
import { EngineInstall } from "../../components/EngineInstall";
import { SkillEditor } from "../../components/SkillEditor";
import { StreamView } from "../../components/StreamView";
import { Banner, Button, Empty, Field, Modal } from "../../components/ui";
import { useI18n } from "../../i18n";
import { api, isEngineMissing } from "../../lib/api";
import type { ProjectDetail, TextEvent } from "../../lib/types";

type Draft = { slug: string; content: string; files: SkillFile[]; adapted: boolean; warnings: string[] };

export function ProjectSkillsTab({ detail, onChanged }: { detail: ProjectDetail; onChanged: () => void }) {
  const { t, err } = useI18n();
  const id = detail.project.id;
  const [skills, setSkills] = useState<Skill[]>([]);
  const [library, setLibrary] = useState<SkillMeta[]>([]);
  const [picking, setPicking] = useState(false);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [adapting, setAdapting] = useState<{ slug: string; instruction: string; running: boolean; text: string; reasoning: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [engineMissing, setEngineMissing] = useState(false);

  const reload = useCallback(async () => {
    try {
      const [own, lib] = await Promise.all([api.listProjectSkills(id), api.listSkills()]);
      setSkills(own);
      setLibrary(lib);
    } catch (e) {
      setError(err(e));
    }
  }, [id, err]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const act = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      await reload();
      onChanged();
    } catch (e) {
      setError(err(e));
    }
  };

  const runAdapt = async () => {
    if (!adapting) return;
    const { slug, instruction } = adapting;
    setAdapting({ ...adapting, running: true, text: "", reasoning: "" });
    const onEvent = (e: TextEvent) =>
      setAdapting((a) => a && (e.type === "token" ? { ...a, text: a.text + e.text } : { ...a, reasoning: a.reasoning + e.text }));
    try {
      const proposal = await api.adaptProjectSkill(id, slug, instruction.trim() || null, onEvent);
      setAdapting(null);
      setDraft({ slug, content: proposal.content, files: proposal.files, adapted: true, warnings: proposal.warnings });
    } catch (e) {
      setAdapting((a) => a && { ...a, running: false });
      setError(err(e));
      setEngineMissing(isEngineMissing(e));
    }
  };

  const attachable = library.filter((s) => !skills.some((own) => own.meta.slug === s.slug));

  return (
    <div className="project-skills">
      {error && (
        <Banner kind="error" onClose={() => setError(null)}>
          {error}
        </Banner>
      )}
      <header className="panel-header">
        <h3>{t("skills.attached")}</h3>
        <Button kind="primary" onClick={() => setPicking(true)} disabled={attachable.length === 0}>
          {t("skills.attach")}
        </Button>
      </header>
      {skills.length === 0 ? (
        <Empty>{t("skills.attached.empty")}</Empty>
      ) : (
        <ul className="cards">
          {skills.map((s) => (
            <li key={s.meta.slug} className="card">
              <div>
                <strong>{s.meta.name}</strong> <span className="muted small">{t("skills.version", { version: s.meta.version })}</span>
                {s.meta.origin.type === "adapted" && <span className="chip">{t("skills.origin.adapted")}</span>}
                <p className="muted small">{s.meta.description}</p>
              </div>
              <div className="inline">
                <Button kind="ghost" onClick={() => setDraft({ slug: s.meta.slug, content: s.content, files: s.files, adapted: false, warnings: [] })}>
                  {t("common.edit")}
                </Button>
                <Button kind="ghost" onClick={() => setAdapting({ slug: s.meta.slug, instruction: "", running: false, text: "", reasoning: "" })}>
                  {t("skills.adapt")}
                </Button>
                <Button kind="danger" onClick={() => act(() => api.detachSkill(id, s.meta.slug))}>
                  {t("skills.detach")}
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}

      {picking && (
        <Modal title={t("skills.attach")} onClose={() => setPicking(false)}>
          <ul className="cards">
            {attachable.map((s) => (
              <li key={s.slug} className="card">
                <div>
                  <strong>{s.name}</strong>
                  <p className="muted small">{s.description}</p>
                </div>
                <Button
                  onClick={() =>
                    act(async () => {
                      await api.attachSkill(id, s.slug);
                      setPicking(false);
                    })
                  }
                >
                  {t("skills.attach")}
                </Button>
              </li>
            ))}
          </ul>
        </Modal>
      )}

      {adapting && (
        <Modal title={t("skills.adapt")} onClose={() => (adapting.running ? undefined : setAdapting(null))} wide>
          <Field label={t("skills.adapt.instruction")}>
            <input
              value={adapting.instruction}
              disabled={adapting.running}
              onChange={(e) => setAdapting({ ...adapting, instruction: e.target.value })}
            />
          </Field>
          <div className="form-actions">
            {adapting.running ? (
              <Button kind="danger" onClick={() => api.cancelOperation(`skill:adapt:${id}:${adapting.slug}`)}>
                {t("agents.cancel")}
              </Button>
            ) : (
              <Button kind="primary" onClick={runAdapt}>
                {t("skills.adapt")}
              </Button>
            )}
          </div>
          {error && (
            <Banner kind="error">
              <p>{error}</p>
              {engineMissing && <EngineInstall onInstalled={() => (setError(null), setEngineMissing(false))} />}
            </Banner>
          )}
          {(adapting.running || adapting.text) && <StreamView text={adapting.text} reasoning={adapting.reasoning} />}
        </Modal>
      )}

      {draft && (
        <Modal title={draft.adapted ? t("skills.proposal") : t("common.edit")} onClose={() => setDraft(null)} wide>
          {draft.warnings.length > 0 && (
            <Banner kind="warning">
              <strong>{t("skills.warnings")}</strong>
              <ul>
                {draft.warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            </Banner>
          )}
          <SkillEditor content={draft.content} files={draft.files} onChange={(content, files) => setDraft({ ...draft, content, files })} />
          <div className="form-actions">
            <Button
              kind="primary"
              onClick={() =>
                act(async () => {
                  await api.updateProjectSkill(id, draft.slug, draft.content, draft.files, draft.adapted);
                  setDraft(null);
                })
              }
            >
              {draft.adapted ? t("skills.apply") : t("common.save")}
            </Button>
            <Button onClick={() => setDraft(null)}>{t("common.cancel")}</Button>
          </div>
        </Modal>
      )}
    </div>
  );
}
