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
});
