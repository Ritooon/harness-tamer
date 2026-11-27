// Téléchargements de modèles en cours, gardés hors des composants : quitter
// la page des réglages ne fait plus oublier un téléchargement qui continue
// en arrière-plan (la page affichait alors « Reprendre… » sur un fichier
// encore en cours d'écriture).
import { useSyncExternalStore } from "react";

import { api } from "./api";
import type { DownloadProgress } from "./types";

type Running = Readonly<Record<string, DownloadProgress>>;

let running: Running = {};
const listeners = new Set<() => void>();

function publish(next: Running) {
  running = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Lance le téléchargement d'un modèle ; sans effet s'il est déjà en cours. */
export async function startModelDownload(id: string): Promise<void> {
  if (running[id]) return;
  publish({ ...running, [id]: { model_id: id, bytes: 0, total: null } });
  try {
    await api.startModelDownload(id, (event) => {
      if (running[id]) publish({ ...running, [id]: event });
    });
  } finally {
    const { [id]: _, ...rest } = running;
    publish(rest);
  }
}

export function useModelDownloads(): Running {
  return useSyncExternalStore(subscribe, () => running);
}
