import { describe, expect, it } from "vitest";

import { href, parseRoute, type Route } from "./router";

describe("routeur", () => {
  it("reconnaît chaque page", () => {
    expect(parseRoute("")).toEqual({ name: "home" });
    expect(parseRoute("#/new")).toEqual({ name: "new" });
    expect(parseRoute("#/project/abc/history")).toEqual({ name: "project", id: "abc", tab: "history" });
    expect(parseRoute("#/project/abc/edit")).toEqual({ name: "edit", id: "abc" });
    expect(parseRoute("#/skills")).toEqual({ name: "skills" });
    expect(parseRoute("#/skills/review-pr")).toEqual({ name: "skills", slug: "review-pr" });
    expect(parseRoute("#/settings")).toEqual({ name: "settings" });
  });

  it("retombe sur l'onglet AGENTS.md ou l'accueil", () => {
    expect(parseRoute("#/project/abc/unknown")).toEqual({ name: "project", id: "abc", tab: "agents" });
    expect(parseRoute("#/project")).toEqual({ name: "home" });
    expect(parseRoute("#/nimporte/quoi")).toEqual({ name: "home" });
  });

  it("fait l'aller-retour, identifiants encodés compris", () => {
    const routes: Route[] = [
      { name: "home" },
      { name: "new" },
      { name: "project", id: "a b/é", tab: "export" },
      { name: "edit", id: "x" },
      { name: "skills", slug: "s" },
      { name: "skills" },
      { name: "settings" },
    ];
    for (const route of routes) expect(parseRoute(href(route))).toEqual(route);
  });
});
