import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { callsOf, mockCommands } from "../test/tauri";
import { MarkdownPreview } from "./MarkdownPreview";

describe("MarkdownPreview", () => {
  it("passe les liens https par la commande de consentement", async () => {
    mockCommands({ open_external_link: false });
    render(<MarkdownPreview source="[doc](https://example.org/a)" />);
    await userEvent.click(screen.getByRole("link", { name: "doc" }));
    expect(callsOf("open_external_link")).toEqual([{ url: "https://example.org/a" }]);
  });

  it("n'ouvre jamais un lien non https", async () => {
    mockCommands({});
    render(<MarkdownPreview source="[x](http://example.org) [y](file:///etc/passwd)" />);
    await userEvent.click(screen.getByRole("link", { name: "x" }));
    await userEvent.click(screen.getByText("y"));
    expect(callsOf("open_external_link")).toHaveLength(0);
  });

  it("ignore le HTML brut et ne charge aucune image distante", () => {
    const { container } = render(
      <MarkdownPreview source={'<script>alert(1)</script>\n\n<img src="https://t.example/p.gif">\n\n![logo](https://cdn.example/logo.png)'} />,
    );
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("[logo]")).toBeInTheDocument();
  });

  it("n'affiche pas non plus les images en data:", () => {
    const { container } = render(<MarkdownPreview source="![p](data:image/png;base64,iVBORw0KGgo=)" />);
    expect(container.querySelector("img")).toBeNull();
  });
});
