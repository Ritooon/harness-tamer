import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { callsOf, mockCommands } from "../test/tauri";
import { emptyInput, ProjectForm, validate } from "./ProjectForm";

describe("validate", () => {
  it("exige un nom", () => {
    expect(validate(emptyInput("fr"))).toEqual({ name: "form.name.required" });
  });

  it("exige un dépôt quand le code existant est coché", () => {
    const input = { ...emptyInput("fr"), name: "X", existing_code: { enabled: true, repo_path: null } };
    expect(validate(input)).toEqual({ repo: "form.repo_path.required" });
    expect(validate({ ...input, existing_code: { enabled: true, repo_path: "/src/x" } })).toEqual({});
  });
});

describe("ProjectForm", () => {
  it("n'appelle pas le cœur tant que le formulaire est invalide", async () => {
    mockCommands({});
    render(<ProjectForm defaultLang="fr" />);
    await userEvent.click(screen.getByRole("button", { name: "Créer le projet" }));
    expect(await screen.findByText("Le nom est obligatoire.")).toBeInTheDocument();
    expect(callsOf("create_project")).toHaveLength(0);
  });

  it("crée le projet puis ouvre sa page", async () => {
    mockCommands({ create_project: ({ input }) => ({ ...(input as object), id: "new-id" }) });
    render(<ProjectForm defaultLang="en" />);
    await userEvent.type(screen.getByLabelText("Nom"), "Atelier");
    await userEvent.click(screen.getByRole("button", { name: "Créer le projet" }));
    await screen.findByRole("button", { name: "Créer le projet" });
    const [call] = callsOf("create_project");
    expect(call?.input).toMatchObject({ name: "Atelier", output_language: "en" });
    expect(window.location.hash).toMatch(/^#\/project\/new-id\//);
  });
});
