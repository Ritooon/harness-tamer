// Accueil : projets actifs ou archivés, recherche, tri, archivage, suppression.
import { useCallback, useEffect, useState } from "react";

import type { ProjectSort } from "../bindings/ProjectSort";
import type { ProjectStatus } from "../bindings/ProjectStatus";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import { Banner, Button, Empty } from "../components/ui";
import { useI18n } from "../i18n";
import type { MessageKey } from "../i18n/fr";
import { api } from "../lib/api";
import { formatDate } from "../lib/format";
import { navigate } from "../lib/router";

type Filter = ProjectStatus | "all";

export function Home() {
  const { t, lang, err } = useI18n();
  const [text, setText] = useState("");
  const [filter, setFilter] = useState<Filter>("active");
  const [sort, setSort] = useState<ProjectSort>("updated_desc");
  const [projects, setProjects] = useState<ProjectSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    api
      .listProjects({ text: text.trim() || null, status: filter === "all" ? null : filter, sort })
      .then(setProjects)
      .catch((e: unknown) => setError(err(e)));
  }, [text, filter, sort, err]);

  useEffect(() => {
    const timer = window.setTimeout(load, 150);
    return () => window.clearTimeout(timer);
  }, [load]);

  const act = (action: Promise<unknown>) => action.then(load).catch((e: unknown) => setError(err(e)));

  return (
    <section className="page">
      <header className="page-header">
        <h1>{t("home.title")}</h1>
        <Button kind="primary" onClick={() => navigate({ name: "new" })}>
          {t("home.new")}
        </Button>
      </header>
      {error && (
        <Banner kind="error" onClose={() => setError(null)}>
          {error}
        </Banner>
      )}
      <div className="toolbar">
        <input
          className="search"
          type="search"
          placeholder={t("home.search")}
          aria-label={t("common.search")}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="segmented" role="group">
          {(["active", "archived", "all"] as Filter[]).map((f) => (
            <button key={f} type="button" aria-pressed={filter === f} className={filter === f ? "on" : ""} onClick={() => setFilter(f)}>
              {t(`home.filter.${f}` as MessageKey)}
            </button>
          ))}
        </div>
        <select aria-label="sort" value={sort} onChange={(e) => setSort(e.target.value as ProjectSort)}>
          <option value="updated_desc">{t("home.sort.updated")}</option>
          <option value="created_desc">{t("home.sort.created")}</option>
          <option value="name_asc">{t("home.sort.name")}</option>
        </select>
      </div>
      {projects === null ? (
        <p>{t("common.loading")}</p>
      ) : projects.length === 0 ? (
        <Empty>{text || filter !== "active" ? t("home.no_match") : t("home.empty")}</Empty>
      ) : (
        <ul className="cards">
          {projects.map((p) => (
            <li key={p.id} className="card">
              <a className="card-main" href={`#/project/${p.id}/agents`}>
                <h2>{p.name}</h2>
                {p.description && <p className="muted clamp">{p.description}</p>}
                <div className="chips">
                  <span className="chip">{t(`platform.${p.platform}` as MessageKey)}</span>
                  {p.imposed_tech.slice(0, 5).map((tech) => (
                    <span key={tech} className="chip chip-outline">
                      {tech}
                    </span>
                  ))}
                  {p.status === "archived" && <span className="chip chip-warning">{t("home.archived_badge")}</span>}
                  <span className={`chip ${p.has_agents ? "chip-ok" : "chip-muted"}`}>
                    {p.has_agents ? t("home.has_agents") : t("home.no_agents")}
                  </span>
                </div>
                <p className="muted small">{t("home.updated", { date: formatDate(p.updated_at, lang) })}</p>
              </a>
              <div className="card-actions">
                {p.status === "active" ? (
                  <Button kind="ghost" onClick={() => act(api.setProjectStatus(p.id, "archived"))}>
                    {t("home.archive")}
                  </Button>
                ) : (
                  <Button kind="ghost" onClick={() => act(api.setProjectStatus(p.id, "active"))}>
                    {t("home.restore")}
                  </Button>
                )}
                <Button kind="danger" onClick={() => act(api.deleteProject(p.id))}>
                  {t("common.delete")}
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
