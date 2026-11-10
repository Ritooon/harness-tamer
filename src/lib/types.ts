// Types des commandes propres à l'application Tauri (src-tauri), écrits à la
// main. Les types du cœur sont générés par ts-rs dans `src/bindings`.
import type { BrandKit } from "../bindings/BrandKit";
import type { HardwareInfo } from "../bindings/HardwareInfo";
import type { Project } from "../bindings/Project";
import type { Recommendation } from "../bindings/Recommendation";
import type { RepoProfile } from "../bindings/RepoProfile";
import type { RuntimeBackend } from "../bindings/RuntimeBackend";
import type { ServerCandidate } from "../bindings/ServerCandidate";
import type { SkillMeta } from "../bindings/SkillMeta";
import type { VersionMeta } from "../bindings/VersionMeta";

export type CmdError = { code: string; message: string };

export type ProjectDetail = {
  project: Project;
  agents: string | null;
  versions: VersionMeta[];
  skills: SkillMeta[];
  repo_profile: RepoProfile | null;
  brand_kit: BrandKit | null;
};

export type RuntimeStatus =
  | { state: "stopped" }
  | { state: "starting"; model: string }
  | { state: "ready"; model: string; backend: RuntimeBackend }
  | { state: "failed"; code: string; message: string };

/** Où se trouve `llama-server` (commande `locate_runtime`). */
export type RuntimeLocation = {
  found: ServerCandidate | null;
  searched: string[];
  install_target: string | null;
};

export type SystemInfo = {
  hardware: HardwareInfo;
  recommendation: Recommendation;
  data_dir: string;
  models_dir: string;
  version: string;
};

export type DownloadProgress = { model_id: string; bytes: number; total: number | null };

export type TextEvent = { type: "reasoning"; text: string } | { type: "token"; text: string };

export type ExportKind = "folder" | "zip";

/** Ligne de `network.log.jsonl` (hf_net::JournalEntry). */
export type JournalEntry = {
  ts: string;
  action_id: string;
  action: "model_download" | "runtime_download" | "skill_search" | "skill_import" | "open_link";
  event: "decision" | "request" | "blocked";
  decision?: "accepted" | "refused" | "blocked_by_policy" | null;
  purpose?: string | null;
  hosts?: string[];
  method?: string | null;
  url?: string | null;
  status?: number | null;
  bytes?: number | null;
  error?: string | null;
};
