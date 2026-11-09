// Page d'un projet : onglets AGENTS.md, historique, skills, contexte, export.
import { useCallback, useEffect, useState } from "react";

import { Banner, Button, Tabs } from "../components/ui";
import { useI18n } from "../i18n";
import type { MessageKey } from "../i18n/fr";
import { api } from "../lib/api";
import { navigate, type ProjectTab } from "../lib/router";
import type { ProjectDetail } from "../lib/types";
import { AgentsTab } from "./project/AgentsTab";
import { ContextTab } from "./project/ContextTab";
import { ExportTab } from "./project/ExportTab";
import { HistoryTab } from "./project/HistoryTab";
import { ProjectSkillsTab } from "./project/ProjectSkillsTab";

const TABS: ProjectTab[] = ["agents", "history", "skills", "context", "export"];

export function ProjectPage({ id, tab }: { id: string; tab: ProjectTab }) {
  const { t, err } = useI18n();
  const [detail, setDetail] = useState<ProjectDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setDetail(await api.getProject(id));
      setError(null);
    } catch (e) {
      setError(err(e));
    }
  }, [id, err]);

  useEffect(() => {
    void reload();
  }, [reload]);

  if (error && !detail) return <Banner kind="error">{error}</Banner>;
  if (!detail) return <p className="muted">{t("common.loading")}</p>;
  const { project } = detail;
  const archived = project.status === "archived";

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <a href="#/" className="muted small">
            ← {t("home.title")}
          </a>
          <h1>{project.name}</h1>
          {project.description && <p className="muted">{project.description}</p>}
        </div>
        <div className="inline">
          <Button onClick={() => navigate({ name: "edit", id })}>{t("common.edit")}</Button>
          <Button
            kind="ghost"
            onClick={async () => {
              await api.setProjectStatus(id, archived ? "active" : "archived");
              await reload();
            }}
          >
            {archived ? t("home.restore") : t("home.archive")}
          </Button>
          <Button
            kind="danger"
            onClick={async () => {
              if (await api.deleteProject(id)) navigate({ name: "home" });
            }}
          >
            {t("common.delete")}
          </Button>
        </div>
      </header>
      {archived && <Banner kind="info">{t("project.archived")}</Banner>}
      {error && (
        <Banner kind="error" onClose={() => setError(null)}>
          {error}
        </Banner>
      )}
      <Tabs
        tabs={TABS.map((id) => ({ id, label: t(`project.tab.${id}` as MessageKey) }))}
        active={tab}
        onChange={(next) => navigate({ name: "project", id, tab: next })}
      />
      <div className="tab-panel" role="tabpanel">
        {tab === "agents" && <AgentsTab detail={detail} onChanged={reload} />}
        {tab === "history" && <HistoryTab detail={detail} onChanged={reload} />}
        {tab === "skills" && <ProjectSkillsTab detail={detail} onChanged={reload} />}
        {tab === "context" && <ContextTab detail={detail} onChanged={reload} onDetail={setDetail} />}
        {tab === "export" && <ExportTab detail={detail} />}
      </div>
    </div>
  );
}
