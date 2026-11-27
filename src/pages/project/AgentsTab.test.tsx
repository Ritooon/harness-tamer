import { EditorView } from "@codemirror/view";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { detail, version } from "../../test/fixtures";
import { callsOf, mockCommands } from "../../test/tauri";
import { AgentsTab } from "./AgentsTab";

const AGENTS = "# Boutique\n\nInstructions pour les agents.\n";

/** Tape dans l'éditeur CodeMirror comme le ferait l'utilisateur. */
function typeInEditor(text: string) {
  const dom = document.querySelector<HTMLElement>(".cm-editor");
  const view = dom && EditorView.findFromDOM(dom);
  if (!view) throw new Error("éditeur introuvable");
  act(() => view.dispatch({ changes: { from: view.state.doc.length, insert: text } }));
}

describe("AgentsTab", () => {
  it("explique pourquoi « Enregistrer la version » est grisé quand rien n'a changé", () => {
    mockCommands({});
    render(<AgentsTab detail={detail({ agents: AGENTS, versions: [version({ number: 2 }), version({ number: 3 })] })} onChanged={() => undefined} />);

    const chip = screen.getByText("À jour · version 3");
    const save = screen.getByRole("button", { name: "Enregistrer la version" });
    const hint = "Aucune modification depuis la version 3. Pour enregistrer une nouvelle version, modifiez le texte en mode Édition ou Côte à côte.";
    expect(save).toBeDisabled();
    expect(save).toHaveAttribute("title", hint);
    expect(chip).toHaveAttribute("title", hint);
    expect(screen.getByRole("textbox", { name: "Note de version (facultative)" })).toBeDisabled();
    expect(screen.queryByText("Modifications non enregistrées")).toBeNull();
  });

  it("active le bouton et la note après une modification, puis repasse à jour", async () => {
    mockCommands({
      save_agents: version({ number: 4, origin: "manual_edit", note: "relu" }),
      check_agents: [],
    });
    const onChanged = vi.fn();
    render(<AgentsTab detail={detail({ agents: AGENTS, versions: [version({ number: 3 })] })} onChanged={onChanged} />);

    typeInEditor("\nAjout.\n");
    const save = screen.getByRole("button", { name: "Enregistrer la version" });
    const note = screen.getByRole("textbox", { name: "Note de version (facultative)" });
    expect(screen.getByText("Modifications non enregistrées")).toBeInTheDocument();
    expect(screen.queryByText(/À jour/)).toBeNull();
    expect(save).toBeEnabled();
    expect(save).not.toHaveAttribute("title");
    expect(note).toBeEnabled();

    await userEvent.type(note, "relu");
    await userEvent.click(save);

    expect(await screen.findByText("À jour · version 4")).toBeInTheDocument();
    expect(callsOf("save_agents")).toEqual([{ id: "p1", content: `${AGENTS}\nAjout.\n`, note: "relu" }]);
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(save).toBeDisabled();
    expect(note).toBeDisabled();
    expect(note).toHaveValue("");
  });

  it("n'affiche pas de pastille sans version enregistrée", () => {
    mockCommands({});
    render(<AgentsTab detail={detail({ agents: AGENTS, versions: [] })} onChanged={() => undefined} />);
    expect(screen.queryByText(/À jour/)).toBeNull();
    expect(screen.getByRole("button", { name: "Enregistrer la version" })).toBeDisabled();
  });
});
