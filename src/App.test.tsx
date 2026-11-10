import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { App } from "./App";
import { settings } from "./test/fixtures";
import { callsOf, mockCommands } from "./test/tauri";

describe("App", () => {
  it("démarre dans la langue des réglages, hors ligne", async () => {
    mockCommands({ get_settings: settings({ ui_language: "en" }), runtime_status: { state: "stopped" }, list_projects: [] });
    render(<App />);
    expect(await screen.findByRole("link", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByText("Offline")).toBeInTheDocument();
    expect(screen.getByText("Model stopped")).toBeInTheDocument();
    await waitFor(() => expect(document.documentElement.lang).toBe("en"));
  });

  it("signale le réseau coupé", async () => {
    mockCommands({ get_settings: settings({ network_policy: "off" }), runtime_status: { state: "stopped" }, list_projects: [] });
    render(<App />);
    expect(await screen.findByText("Coupé : aucun accès à Internet")).toBeInTheDocument();
  });

  it("propose d'installer le moteur manquant, sans console", async () => {
    let status: object = { state: "failed", code: "runtime.missing", message: "/a/llama-server\n/b/llama-server" };
    mockCommands({
      get_settings: settings(),
      runtime_status: () => status,
      list_projects: [],
      install_llama_runtime: () => {
        status = { state: "stopped" };
        return { tag: "v0.9.0", target: "macos-arm64/metal", server: "/d/llama-server", files: ["llama-server"] };
      },
    });
    render(<App />);
    expect(await screen.findByText(/Le moteur llama.cpp, qui fait tourner le modèle, n'est pas encore installé/)).toBeInTheDocument();
    // Les chemins techniques ne sont pas jetés au visage de l'utilisateur.
    expect(screen.queryByText(/\/a\/llama-server/)).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Installer le moteur llama.cpp…" }));
    expect(await screen.findByText(/Moteur llama.cpp v0.9.0 installé/)).toBeInTheDocument();
    expect(callsOf("install_llama_runtime")).toHaveLength(1);
  });
});
