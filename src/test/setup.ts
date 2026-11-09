// Préparation des tests : matchers DOM et faux pont Tauri. Aucun test ne
// parle à un vrai processus Rust ; chaque test décide des réponses.
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: (message: T) => void = () => undefined;
  }
  return { invoke: vi.fn(), Channel };
});

// jsdom n'implémente pas toujours <dialog>.showModal().
if (typeof HTMLDialogElement !== "undefined" && !HTMLDialogElement.prototype.showModal) {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute("open");
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  window.location.hash = "";
});
