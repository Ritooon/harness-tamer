import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { summary } from "../test/fixtures";
import { callsOf, mockCommands } from "../test/tauri";
import { Home } from "./Home";

describe("Home", () => {
  it("affiche les projets et invite à créer le premier", async () => {
    mockCommands({ list_projects: [] });
    render(<Home />);
    expect(await screen.findByText(/Aucun projet pour l'instant/)).toBeInTheDocument();
  });

  it("liste, cherche et archive", async () => {
    mockCommands({
      list_projects: [summary(), summary({ id: "p2", name: "Outil CLI", platform: "cli", has_agents: true })],
      set_project_status: ({ id, status }) => ({ id, status }),
    });
    render(<Home />);
    expect(await screen.findByText("Boutique")).toBeInTheDocument();
    expect(screen.getByText("Outil CLI")).toBeInTheDocument();
    expect(screen.getByText("AGENTS.md prêt")).toBeInTheDocument();

    await userEvent.type(screen.getByRole("searchbox"), "rust");
    await waitFor(() => expect(callsOf("list_projects").at(-1)?.query).toMatchObject({ text: "rust", status: "active" }));

    const [archive] = screen.getAllByRole("button", { name: "Archiver" });
    await userEvent.click(archive!);
    await waitFor(() => expect(callsOf("set_project_status")).toEqual([{ id: "p1", status: "archived" }]));
  });

  it("affiche une erreur du cœur de façon lisible", async () => {
    mockCommands({ list_projects: () => Promise.reject({ code: "project.not_found", message: "" }) });
    render(<Home />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Projet introuvable.");
  });
});
