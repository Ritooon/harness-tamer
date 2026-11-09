import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { App } from "./App";
import { settings } from "./test/fixtures";
import { mockCommands } from "./test/tauri";

describe("App", () => {
  it("démarre dans la langue des réglages, hors ligne", async () => {
    mockCommands({ get_settings: settings({ ui_language: "en" }), runtime_status: { state: "stopped" }, list_projects: [] });
    render(<App />);
    expect(await screen.findByRole("link", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByText("Offline")).toBeInTheDocument();
    expect(screen.getByText("Model stopped")).toBeInTheDocument();
    expect(document.documentElement.lang).toBe("en");
  });

  it("signale le réseau coupé", async () => {
    mockCommands({ get_settings: settings({ network_policy: "off" }), runtime_status: { state: "stopped" }, list_projects: [] });
    render(<App />);
    expect(await screen.findByText("Coupé : aucun accès à Internet")).toBeInTheDocument();
  });
});
