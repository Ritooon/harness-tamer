// Coquille de l'application : navigation, état du modèle, badge hors ligne.
// Les réglages sont lus avant tout rendu pour fixer la langue de l'interface.
import { useCallback, useEffect, useState } from "react";

import type { Settings } from "./bindings/Settings";
import { Banner } from "./components/ui";
import { errorMessage, I18nProvider, useI18n } from "./i18n";
import { api } from "./lib/api";
import { useRoute, type Route } from "./lib/router";
import type { RuntimeStatus } from "./lib/types";
import { Home } from "./pages/Home";
import { ProjectForm } from "./pages/ProjectForm";
import { ProjectPage } from "./pages/ProjectPage";
import { RuntimeBadge, SettingsPage } from "./pages/SettingsPage";
import { SkillsPage } from "./pages/SkillsPage";

export function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    api.getSettings().then(setSettings).catch(setError);
  }, []);

  useEffect(() => {
    if (settings) document.documentElement.lang = settings.ui_language;
  }, [settings]);

  if (error) return <p role="alert">{errorMessage("fr", error)}</p>;
  if (!settings) return null;
  return (
    <I18nProvider lang={settings.ui_language}>
      <Shell settings={settings} onSettings={setSettings} />
    </I18nProvider>
  );
}

function useRuntimeStatus(): RuntimeStatus {
  const [status, setStatus] = useState<RuntimeStatus>({ state: "stopped" });
  useEffect(() => {
    const poll = () => api.runtimeStatus().then(setStatus).catch(() => undefined);
    poll();
    const timer = window.setInterval(poll, 3000);
    return () => window.clearInterval(timer);
  }, []);
  return status;
}

function Shell({ settings, onSettings }: { settings: Settings; onSettings: (s: Settings) => void }) {
  const { t } = useI18n();
  const route = useRoute();
  const status = useRuntimeStatus();
  const section = route.name === "skills" ? "skills" : route.name === "settings" ? "settings" : "projects";
  const offline = settings.network_policy === "off";

  return (
    <div className="app">
      <a className="skip" href="#main">
        {t("app.skip")}
      </a>
      <aside className="sidebar">
        <div className="brand">
          <span className="logo" aria-hidden="true">
            ⚒
          </span>
          {t("app.name")}
        </div>
        <nav aria-label={t("app.name")}>
          <a href="#/" aria-current={section === "projects" ? "page" : undefined}>
            {t("nav.projects")}
          </a>
          <a href="#/skills" aria-current={section === "skills" ? "page" : undefined}>
            {t("nav.skills")}
          </a>
          <a href="#/settings" aria-current={section === "settings" ? "page" : undefined}>
            {t("nav.settings")}
          </a>
        </nav>
        <div className="sidebar-footer">
          <a href="#/settings" className="status-link">
            <RuntimeBadge status={status} />
          </a>
          <span className={`pill ${offline ? "pill-muted" : "pill-success"}`} title={t("app.offline.hint")}>
            {offline ? t("settings.network.off") : t("app.offline")}
          </span>
        </div>
      </aside>
      <main id="main" className="main">
        {status.state === "failed" && route.name !== "settings" && <Banner kind="error">{status.message}</Banner>}
        <Page route={route} settings={settings} onSettings={onSettings} />
      </main>
    </div>
  );
}

function Page({ route, settings, onSettings }: { route: Route; settings: Settings; onSettings: (s: Settings) => void }) {
  const save = useCallback((s: Settings) => onSettings(s), [onSettings]);
  switch (route.name) {
    case "home":
      return <Home />;
    case "new":
      return <ProjectForm defaultLang={settings.default_output_language} />;
    case "edit":
      return <ProjectForm key={route.id} id={route.id} defaultLang={settings.default_output_language} />;
    case "project":
      return <ProjectPage key={route.id} id={route.id} tab={route.tab} />;
    case "skills":
      return <SkillsPage slug={route.slug} />;
    case "settings":
      return <SettingsPage settings={settings} onSettings={save} />;
  }
}
