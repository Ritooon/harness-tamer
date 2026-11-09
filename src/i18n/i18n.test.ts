import { describe, expect, it } from "vitest";

import { en } from "./en";
import { fr, type MessageKey } from "./fr";
import { errorMessage, format, translate } from ".";

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("dictionnaires", () => {
  it("ont exactement les mêmes clés", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(fr).sort());
  });

  it("n'ont aucun texte vide", () => {
    for (const dict of [fr, en]) {
      for (const [key, value] of Object.entries(dict)) expect(value.trim(), key).not.toBe("");
    }
  });

  it("utilisent les mêmes variables dans les deux langues", () => {
    for (const key of Object.keys(fr) as MessageKey[]) {
      expect(placeholders(en[key]), key).toEqual(placeholders(fr[key]));
    }
  });
});

describe("format et traduction", () => {
  it("remplace les variables connues et garde les autres", () => {
    expect(format("{a} et {b}", { a: 1 })).toBe("1 et {b}");
  });

  it("traduit selon la langue", () => {
    expect(translate("fr", "nav.settings")).toBe("Réglages");
    expect(translate("en", "nav.settings")).toBe("Settings");
    expect(translate("en", "history.version", { number: 3 })).toBe("Version 3");
  });
});

describe("messages d'erreur", () => {
  it("traduit un code d'erreur connu", () => {
    expect(errorMessage("fr", { code: "net.refused", message: "x" })).toBe("Accès à Internet refusé.");
  });

  it("retombe sur le message générique pour un code inconnu", () => {
    expect(errorMessage("en", { code: "nope", message: "boom" })).toBe("Something went wrong: boom");
  });

  it("accepte une erreur JavaScript quelconque", () => {
    expect(errorMessage("fr", new Error("panne"))).toContain("panne");
  });
});
