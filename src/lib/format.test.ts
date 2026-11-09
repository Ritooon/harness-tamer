import { describe, expect, it } from "vitest";

import { formatBytes, formatDate, stripMarkers } from "./format";

describe("formatBytes", () => {
  it("utilise les unités de la langue", () => {
    expect(formatBytes(512, "fr")).toBe("512 o");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatBytes(22.1 * 1024 ** 3, "fr").replace(/\s/g, " ")).toBe("22,1 Go");
  });
});

describe("formatDate", () => {
  it("rend une date invalide telle quelle", () => {
    expect(formatDate("pas une date", "fr")).toBe("pas une date");
  });
});

describe("stripMarkers", () => {
  it("retire uniquement les marqueurs internes", () => {
    const doc = '# Titre\n\n<!-- hf:section id="stack" -->\n## Stack\n\nRust\n<!-- hf:end -->\n\n<!-- commentaire -->\n';
    expect(stripMarkers(doc)).toBe("# Titre\n\n## Stack\n\nRust\n\n<!-- commentaire -->\n");
  });
});
