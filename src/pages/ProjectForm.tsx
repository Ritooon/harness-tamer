// Formulaire « Nouveau projet » / « Modifier le projet ».
import { useEffect, useState, type FormEvent } from "react";

import type { Lang } from "../bindings/Lang";
import type { Platform } from "../bindings/Platform";
import type { ProjectInput } from "../bindings/ProjectInput";
import type { TargetOs } from "../bindings/TargetOs";
import { TagInput } from "../components/TagInput";
import { Banner, Button, Field } from "../components/ui";
import { useI18n } from "../i18n";
import type { MessageKey } from "../i18n/fr";
import { api } from "../lib/api";
import { navigate } from "../lib/router";

const PLATFORMS: Platform[] = ["web", "mobile", "desktop", "cli", "api", "embedded", "library", "other"];
// Noms propres affichés tels quels ; les autres libellés sont traduits.
const TARGETS: { id: TargetOs; label: string; key?: MessageKey }[] = [
  { id: "windows", label: "Windows" },
  { id: "linux", label: "Linux" },
  { id: "macos", label: "macOS" },
  { id: "android", label: "Android" },
  { id: "ios", label: "iOS" },
  { id: "browser", label: "", key: "target.browser" },
  { id: "other", label: "", key: "target.other" },
];

export function emptyInput(lang: Lang): ProjectInput {
  return {
    name: "",
    description: "",
    platform: "web",
    target_os: [],
    imposed_tech: [],
    existing_code: { enabled: false, repo_path: null },
    brand_enabled: false,
    constraints: "",
    output_language: lang,
  };
}

/** Erreurs de saisie, avant tout appel au cœur (qui revalide). */
export function validate(input: ProjectInput): Partial<Record<"name" | "repo", MessageKey>> {
  const errors: Partial<Record<"name" | "repo", MessageKey>> = {};
  if (!input.name.trim()) errors.name = "form.name.required";
  if (input.existing_code.enabled && !input.existing_code.repo_path?.trim()) errors.repo = "form.repo_path.required";
  return errors;
}

export function ProjectForm({ id, defaultLang }: { id?: string; defaultLang: Lang }) {
  const { t, err } = useI18n();
  const [input, setInput] = useState<ProjectInput>(() => emptyInput(defaultLang));
  const [submitted, setSubmitted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;
    api
      .getProject(id)
      .then(({ project: p }) =>
        setInput({
          name: p.name,
          description: p.description,
          platform: p.platform,
          target_os: p.target_os,
          imposed_tech: p.imposed_tech,
          existing_code: p.existing_code,
          brand_enabled: p.brand.enabled,
          constraints: p.constraints,
          output_language: p.output_language,
        }),
      )
      .catch((e: unknown) => setError(err(e)));
  }, [id, err]);

  const errors = validate(input);
  const set = <K extends keyof ProjectInput>(key: K, value: ProjectInput[K]) => setInput((prev) => ({ ...prev, [key]: value }));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setSubmitted(true);
    if (Object.keys(errors).length > 0) return;
    setBusy(true);
    try {
      const project = id ? await api.updateProject(id, input) : await api.createProject(input);
      if (!id && project.existing_code.enabled) {
        // Analyse locale du dépôt dès la création (aucun réseau).
        await api.analyzeRepo(project.id).catch(() => undefined);
      }
      navigate({ name: "project", id: project.id, tab: id ? "agents" : "context" });
    } catch (e) {
      setError(err(e));
    } finally {
      setBusy(false);
    }
  };

  const pickRepo = async () => {
    const path = await api.pickFolder().catch((e: unknown) => {
      setError(err(e));
      return null;
    });
    if (path) set("existing_code", { enabled: true, repo_path: path });
  };

  return (
    <section className="page narrow">
      <header className="page-header">
        <h1>{id ? t("form.edit_title") : t("form.new_title")}</h1>
      </header>
      {error && (
        <Banner kind="error" onClose={() => setError(null)}>
          {error}
        </Banner>
      )}
      <form className="form" onSubmit={submit} noValidate>
        <Field label={t("form.name")} error={submitted && errors.name ? t(errors.name) : undefined}>
          <input value={input.name} maxLength={120} onChange={(e) => set("name", e.target.value)} autoFocus />
        </Field>
        <Field label={t("form.description")} hint={t("form.description.hint")}>
          <textarea rows={4} maxLength={4000} value={input.description} onChange={(e) => set("description", e.target.value)} />
        </Field>
        <div className="row">
          <Field label={t("form.platform")}>
            <select value={input.platform} onChange={(e) => set("platform", e.target.value as Platform)}>
              {PLATFORMS.map((p) => (
                <option key={p} value={p}>
                  {t(`platform.${p}` as MessageKey)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("form.output_language")}>
            <select value={input.output_language} onChange={(e) => set("output_language", e.target.value as Lang)}>
              <option value="fr">{t("lang.fr")}</option>
              <option value="en">{t("lang.en")}</option>
            </select>
          </Field>
        </div>
        <fieldset className="fieldset">
          <legend>{t("form.target_os")}</legend>
          <div className="checks">
            {TARGETS.map((o) => (
              <label key={o.id} className="check">
                <input
                  type="checkbox"
                  checked={input.target_os.includes(o.id)}
                  onChange={(e) =>
                    set("target_os", e.target.checked ? [...input.target_os, o.id] : input.target_os.filter((x) => x !== o.id))
                  }
                />
                {o.key ? t(o.key) : o.label}
              </label>
            ))}
          </div>
        </fieldset>
        <Field label={t("form.imposed_tech")} hint={t("form.imposed_tech.hint")}>
          <TagInput label={t("form.imposed_tech")} value={input.imposed_tech} onChange={(v) => set("imposed_tech", v)} />
        </Field>
        <fieldset className="fieldset">
          <label className="check">
            <input
              type="checkbox"
              checked={input.existing_code.enabled}
              onChange={(e) => set("existing_code", { ...input.existing_code, enabled: e.target.checked })}
            />
            {t("form.existing_code")}
          </label>
          {input.existing_code.enabled && (
            <Field label={t("form.repo_path")} hint={t("form.repo_path.hint")} error={submitted && errors.repo ? t(errors.repo) : undefined}>
              <div className="inline">
                <input
                  value={input.existing_code.repo_path ?? ""}
                  onChange={(e) => set("existing_code", { enabled: true, repo_path: e.target.value })}
                />
                <Button onClick={pickRepo}>{t("form.repo_path.pick")}</Button>
              </div>
            </Field>
          )}
        </fieldset>
        <fieldset className="fieldset">
          <label className="check">
            <input type="checkbox" checked={input.brand_enabled} onChange={(e) => set("brand_enabled", e.target.checked)} />
            {t("form.brand")}
          </label>
          {input.brand_enabled && <p className="field-hint">{t("form.brand.hint")}</p>}
        </fieldset>
        <Field label={t("form.constraints")} hint={t("form.constraints.hint")}>
          <textarea rows={4} value={input.constraints} onChange={(e) => set("constraints", e.target.value)} />
        </Field>
        <div className="form-actions">
          <Button onClick={() => (id ? navigate({ name: "project", id, tab: "agents" }) : navigate({ name: "home" }))}>
            {t("common.cancel")}
          </Button>
          <Button kind="primary" type="submit" disabled={busy}>
            {id ? t("common.save") : t("form.create")}
          </Button>
        </div>
      </form>
    </section>
  );
}
