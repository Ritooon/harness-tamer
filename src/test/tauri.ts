// Faux `invoke` piloté par une table commande → réponse.
import { invoke } from "@tauri-apps/api/core";
import { vi } from "vitest";

export type Handler = (args: Record<string, unknown>) => unknown;
type Reply = Handler | object | string | number | boolean | null;

export const invokeMock = vi.mocked(invoke);

/** Répond aux commandes listées ; toute autre commande échoue bruyamment. */
export function mockCommands(handlers: Record<string, Reply>) {
  invokeMock.mockImplementation(async (command: string, args?: unknown) => {
    if (!(command in handlers)) throw { code: "test.unmocked", message: `commande non simulée : ${command}` };
    const handler = handlers[command];
    return typeof handler === "function" ? (handler as Handler)((args ?? {}) as Record<string, unknown>) : handler;
  });
}

export function callsOf(command: string): Record<string, unknown>[] {
  return invokeMock.mock.calls.filter(([c]) => c === command).map(([, a]) => (a ?? {}) as Record<string, unknown>);
}
