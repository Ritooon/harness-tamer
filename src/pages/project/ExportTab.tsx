// Export : AGENTS.md sans marqueurs et dossier .agents/ (dossier ou ZIP).
import { useState } from "react";

import type { ExportOptions } from "../../bindings/ExportOptions";
import type { ExportReport } from "../../bindings/ExportReport";
import { Banner, Button, Empty } from "../../components/ui";
import { useI18n } from "../../i18n";
import { api } from "../../lib/api";
import type { ExportKind, ProjectDetail } from "../../lib/types";

export function ExportTab({ detail }: { detail: ProjectDetail }) {
  const { t, err } = useI18n();
  const [options, setOptions] = useState<ExportOptions>({ agents_at_root: true, include_skills: true, overwrite: false });
  const [report, setReport] = useState<ExportReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (!detail.agents) return <Empty>{t("export.no_agents")}</Empty>;

  const run = async (kind: ExportKind) => {
    setError(null);
    try {
      const r = await api.exportProject(detail.project.id, kind, options);
      if (r) setReport(r);
    } catch (e) {
      setError(err(e));
    }
  };

  return (
    <div className="panel narrow">
      <h3>{t("export.title")}</h3>
      <p className="muted">{t("export.hint")}</p>
      <label className="check">
        <input type="checkbox" checked={options.agents_at_root} onChange={(e) => setOptions({ ...options, agents_at_root: e.target.checked })} />
        {t("export.agents_at_root")}
      </label>
      <label className="check">
        <input type="checkbox" checked={options.include_skills} onChange={(e) => setOptions({ ...options, include_skills: e.target.checked })} />
        {t("export.include_skills")}
      </label>
      <div className="form-actions">
        <Button kind="primary" onClick={() => run("folder")}>
          {t("export.to_folder")}
        </Button>
        <Button onClick={() => run("zip")}>{t("export.to_zip")}</Button>
      </div>
      {error && <Banner kind="error">{error}</Banner>}
      {report && (
        <Banner kind="success" onClose={() => setReport(null)}>
          <p>{t("export.done", { count: report.written.length, target: report.target })}</p>
          <ul className="files">
            {report.written.map((f) => (
              <li key={f}>
                <code>{f}</code>
              </li>
            ))}
          </ul>
          <Button kind="ghost" onClick={() => api.revealPath(report.target)}>
            {t("common.open_folder")}
          </Button>
        </Banner>
      )}
    </div>
  );
}
