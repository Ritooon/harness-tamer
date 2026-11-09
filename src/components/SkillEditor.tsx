// Édition d'un skill : SKILL.md et fichiers de ressources (texte).
import { useState } from "react";

import type { SkillFile } from "../bindings/SkillFile";
import { useI18n } from "../i18n";
import { MarkdownEditor } from "./MarkdownEditor";
import { MarkdownPreview } from "./MarkdownPreview";
import { Button, Field, Tabs } from "./ui";

export function SkillEditor({
  content,
  files,
  onChange,
}: {
  content: string;
  files: SkillFile[];
  onChange: (content: string, files: SkillFile[]) => void;
}) {
  const { t } = useI18n();
  const [tab, setTab] = useState<string>("SKILL.md");
  const [newPath, setNewPath] = useState("");
  const current = files.find((f) => f.path === tab);

  const addFile = () => {
    const path = newPath.trim().replace(/^\/+/, "");
    if (!path || path.includes("..") || path === "SKILL.md" || files.some((f) => f.path === path)) return;
    onChange(content, [...files, { path, content: "" }]);
    setTab(path);
    setNewPath("");
  };

  return (
    <div className="skill-editor">
      <p className="field-hint">{t("skills.name_hint")}</p>
      <Tabs
        tabs={[{ id: "SKILL.md", label: "SKILL.md" }, ...files.map((f) => ({ id: f.path, label: f.path }))]}
        active={tab}
        onChange={setTab}
      />
      {tab === "SKILL.md" ? (
        <div className="split">
          <MarkdownEditor label="SKILL.md" value={content} onChange={(v) => onChange(v, files)} />
          <MarkdownPreview source={content.replace(/^---[\s\S]*?\n---\n/, "")} />
        </div>
      ) : current ? (
        <div>
          <textarea
            className="code"
            rows={18}
            aria-label={current.path}
            value={current.content}
            onChange={(e) => onChange(content, files.map((f) => (f.path === current.path ? { ...f, content: e.target.value } : f)))}
          />
          <Button
            kind="danger"
            onClick={() => {
              onChange(content, files.filter((f) => f.path !== current.path));
              setTab("SKILL.md");
            }}
          >
            {t("common.delete")}
          </Button>
        </div>
      ) : null}
      <div className="inline">
        <Field label={t("skills.file_path")}>
          <input value={newPath} placeholder="scripts/check.sh" onChange={(e) => setNewPath(e.target.value)} />
        </Field>
        <Button onClick={addFile}>{t("skills.add_file")}</Button>
      </div>
    </div>
  );
}
