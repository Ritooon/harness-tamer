// Contexte : analyse locale du dépôt et charte graphique importée.
import { useState } from "react";

import { Banner, Button, Empty } from "../../components/ui";
import { useI18n } from "../../i18n";
import { api } from "../../lib/api";
import { formatDate } from "../../lib/format";
import type { ProjectDetail } from "../../lib/types";

export function ContextTab({ detail, onChanged, onDetail }: { detail: ProjectDetail; onChanged: () => void; onDetail: (d: ProjectDetail) => void }) {
  const { t, lang, err } = useI18n();
  const { project, repo_profile: profile, brand_kit: kit } = detail;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(err(e));
    } finally {
      setBusy(false);
    }
  };

  const totalBytes = profile?.languages.reduce((sum, l) => sum + l.bytes, 0) ?? 0;

  return (
    <div className="context">
      {error && (
        <Banner kind="error" onClose={() => setError(null)}>
          {error}
        </Banner>
      )}
      <section className="panel">
        <header className="panel-header">
          <h3>{t("context.repo")}</h3>
          {project.existing_code.enabled && (
            <Button disabled={busy} onClick={() => run(async () => { await api.analyzeRepo(project.id); onChanged(); })}>
              {profile ? t("context.repo.rescan") : t("context.repo.scan")}
            </Button>
          )}
        </header>
        {!project.existing_code.enabled ? (
          <Empty>{t("context.repo.none")}</Empty>
        ) : !profile ? (
          <Empty>{t("context.repo.not_scanned")}</Empty>
        ) : (
          <div className="facts">
            <p className="muted">
              <code>{profile.root}</code> · {t("context.repo.files", { count: profile.file_count })} · {formatDate(profile.scanned_at, lang)}
            </p>
            {profile.languages.length > 0 && (
              <div>
                <h4>{t("context.repo.languages")}</h4>
                <div className="chips">
                  {profile.languages.slice(0, 8).map((l) => (
                    <span key={l.name} className="chip">
                      {l.name} {totalBytes ? Math.round((l.bytes * 100) / totalBytes) : 0} %
                    </span>
                  ))}
                </div>
              </div>
            )}
            {profile.commands.length > 0 && (
              <div>
                <h4>{t("context.repo.commands")}</h4>
                <table className="table">
                  <tbody>
                    {profile.commands.map((c, i) => (
                      <tr key={i}>
                        <td>{c.role}</td>
                        <td>
                          <code>{c.command}</code>
                        </td>
                        <td className="muted">{c.source}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {profile.tools.length > 0 && (
              <div>
                <h4>{t("context.repo.tools")}</h4>
                <div className="chips">
                  {profile.tools.map((tool) => (
                    <span key={tool.name} className="chip chip-outline" title={tool.config}>
                      {tool.name}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {profile.ci.length > 0 && (
              <div>
                <h4>{t("context.repo.ci")}</h4>
                <ul>
                  {profile.ci.map((w) => (
                    <li key={w.path}>
                      <code>{w.path}</code> ({w.provider})
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {profile.git && (
              <div>
                <h4>{t("context.repo.git")}</h4>
                <p>
                  {profile.git.branch && <code>{profile.git.branch}</code>}{" "}
                  {t("context.repo.conventional", { percent: Math.round(profile.git.conventional_ratio * 100) })}
                </p>
              </div>
            )}
          </div>
        )}
      </section>

      <section className="panel">
        <header className="panel-header">
          <h3>{t("context.brand")}</h3>
          {project.brand.enabled && (
            <Button disabled={busy} onClick={() => run(async () => onDetail(await api.importBrandFiles(project.id)))}>
              {t("context.brand.import")}
            </Button>
          )}
        </header>
        {!project.brand.enabled ? (
          <Empty>{t("context.brand.disabled")}</Empty>
        ) : (
          <div className="facts">
            {project.brand.files.length > 0 && (
              <div>
                <h4>{t("context.brand.files")}</h4>
                <ul className="files">
                  {project.brand.files.map((f) => (
                    <li key={f}>
                      <code>{f}</code>
                      <Button kind="ghost" onClick={() => run(async () => onDetail(await api.removeBrandFile(project.id, f)))}>
                        {t("common.delete")}
                      </Button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {kit && kit.colors.length > 0 && (
              <div>
                <h4>{t("context.brand.colors")}</h4>
                <div className="swatches">
                  {kit.colors.map((c) => (
                    <span key={c.value} className="swatch" title={c.source}>
                      <span className="swatch-color" style={{ background: c.value }} />
                      <code>{c.value}</code> {c.name}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {kit && kit.fonts.length > 0 && (
              <p>
                <strong>{t("context.brand.fonts")} :</strong> {kit.fonts.map((f) => f.family).join(", ")}
              </p>
            )}
            {kit && kit.logos.length > 0 && (
              <p>
                <strong>{t("context.brand.logos")} :</strong> {kit.logos.join(", ")}
              </p>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
