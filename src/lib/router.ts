// Routeur minimal par hash (#/…) : pas de dépendance, pas d'historique serveur.
import { useEffect, useState } from "react";

export type Route =
  | { name: "home" }
  | { name: "new" }
  | { name: "project"; id: string; tab: ProjectTab }
  | { name: "edit"; id: string }
  | { name: "skills"; slug?: string }
  | { name: "settings" };

export type ProjectTab = "agents" | "history" | "skills" | "context" | "export";
const TABS: ProjectTab[] = ["agents", "history", "skills", "context", "export"];

export function parseRoute(hash: string): Route {
  const parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean).map(decodeURIComponent);
  switch (parts[0]) {
    case "new":
      return { name: "new" };
    case "project": {
      const id = parts[1];
      if (!id) return { name: "home" };
      if (parts[2] === "edit") return { name: "edit", id };
      const tab = TABS.find((t) => t === parts[2]) ?? "agents";
      return { name: "project", id, tab };
    }
    case "skills":
      return parts[1] ? { name: "skills", slug: parts[1] } : { name: "skills" };
    case "settings":
      return { name: "settings" };
    default:
      return { name: "home" };
  }
}

export function href(route: Route): string {
  switch (route.name) {
    case "home":
      return "#/";
    case "new":
      return "#/new";
    case "project":
      return `#/project/${encodeURIComponent(route.id)}/${route.tab}`;
    case "edit":
      return `#/project/${encodeURIComponent(route.id)}/edit`;
    case "skills":
      return route.slug ? `#/skills/${encodeURIComponent(route.slug)}` : "#/skills";
    case "settings":
      return "#/settings";
  }
}

export function navigate(route: Route): void {
  window.location.hash = href(route);
}

export function useRoute(): Route {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.hash));
  useEffect(() => {
    const onChange = () => setRoute(parseRoute(window.location.hash));
    window.addEventListener("hashchange", onChange);
    return () => window.removeEventListener("hashchange", onChange);
  }, []);
  return route;
}
