import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { translate } from "../i18n";
import type { MessageKey } from "../i18n/fr";
import { settings } from "../test/fixtures";
import { callsOf, mockCommands } from "../test/tauri";
import { journalSummary, SettingsPage } from "./SettingsPage";

const t = (key: MessageKey, vars?: Record<string, string | number>) => translate("fr", key, vars);

describe("journalSummary", () => {
  it("décrit une décision, une requête et un blocage", () => {
    expect(
      journalSummary({ ts: "", action_id: "a", action: "skill_search", event: "decision", decision: "refused", purpose: "Chercher" }, t),
    ).toBe("Refusé · Chercher");
    expect(
      journalSummary({ ts: "", action_id: "a", action: "skill_search", event: "request", method: "GET", url: "https://x/y", status: 200 }, t),
    ).toBe("GET https://x/y → 200");
    expect(journalSummary({ ts: "", action_id: "a", action: "open_link", event: "blocked", url: "https://z" }, t)).toBe(
      "Requête bloquée · https://z",
    );
  });
});

describe("SettingsPage", () => {
  it("enregistre la politique réseau et montre le journal", async () => {
    const saved: unknown[] = [];
    mockCommands({
      system_info: {
        hardware: { os: "linux", arch: "x86_64", total_memory_bytes: 32 * 1024 ** 3, available_memory_bytes: 0, cpu_threads: 16, unified_memory: false },
        recommendation: { model_id: "qwen3.6-35b-a3b", below_minimum: false },
        data_dir: "/data",
        models_dir: "/data/models",
        version: "0.1.0",
      },
      list_models: [],
      runtime_status: { state: "stopped" },
      locate_runtime: { found: null, searched: [], install_target: null },
      read_network_log: [
        { ts: "2026-11-09T10:00:00Z", action_id: "1", action: "model_download", event: "decision", decision: "accepted", purpose: "Télécharger" },
      ],
      save_settings: ({ settings: s }) => (saved.push(s), s),
    });
    render(<SettingsPage settings={settings()} onSettings={() => undefined} />);
    expect(await screen.findByText("Téléchargement de modèle")).toBeInTheDocument();
    expect(screen.getByText("Autorisé · Télécharger")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("Coupé : aucun accès à Internet"));
    await waitFor(() => expect(callsOf("save_settings")).toHaveLength(1));
    expect(saved[0]).toMatchObject({ network_policy: "off" });
  });

  const base = {
    system_info: {
      hardware: { os: "macos", arch: "aarch64", total_memory_bytes: 1, available_memory_bytes: 0, cpu_threads: 8, unified_memory: true },
      recommendation: { model_id: "m", below_minimum: false },
      data_dir: "/data",
      models_dir: "/data/models",
      version: "0.1.0",
    },
    list_models: [],
    read_network_log: [],
  };

  it("explique un moteur absent : emplacements consultés, installation en un clic, journal", async () => {
    mockCommands({
      ...base,
      runtime_status: { state: "failed", code: "runtime.missing", message: "/opt/homebrew/bin/llama-server" },
      locate_runtime: { found: null, searched: ["/data/runtimes/llama/llama-server", "/opt/homebrew/bin/llama-server"], install_target: "macos-arm64/metal" },
      start_runtime: () => Promise.reject({ code: "runtime.missing", message: "/opt/homebrew/bin/llama-server" }),
      runtime_log: ["[HarnessForge] Démarrage de llama.cpp avec le modèle m.", "[HarnessForge] Échec : [runtime.missing] …"],
    });
    render(<SettingsPage settings={settings({ active_model: "m" })} onSettings={() => undefined} />);
    expect(await screen.findByText("Non installé")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Installer le moteur llama.cpp…" })).toBeInTheDocument();
    expect(screen.getByText("/opt/homebrew/bin/llama-server")).toBeInTheDocument();
    expect(screen.getByText(/Il n'a été trouvé ni dans l'application ni sur le système/)).toBeInTheDocument();
    // Pas de second message d'erreur pour la même cause.
    expect(screen.queryByText(/n'est pas encore installé sur cet ordinateur/)).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Charger le modèle" }));
    expect(await screen.findByText(/Démarrage de llama.cpp avec le modèle m/)).toBeInTheDocument();
  });

  it("montre le programme trouvé et permet d'en choisir un autre", async () => {
    const saved: unknown[] = [];
    mockCommands({
      ...base,
      runtime_status: { state: "stopped" },
      locate_runtime: { found: { path: "/opt/homebrew/bin/llama-server", source: "system" }, searched: [], install_target: "macos-arm64/metal" },
      pick_llama_server: "/custom/llama-server",
      save_settings: ({ settings: s }) => (saved.push(s), s),
    });
    render(<SettingsPage settings={settings()} onSettings={() => undefined} />);
    expect(await screen.findByText("/opt/homebrew/bin/llama-server")).toBeInTheDocument();
    expect(screen.getByText("installé sur le système")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Installer le moteur llama.cpp…" })).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Choisir le programme…" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({ runtime: { server_path: "/custom/llama-server" } });
  });
});
