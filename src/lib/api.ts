// Accès typé aux commandes Rust. L'interface n'a aucun autre moyen d'agir
// sur la machine : ni réseau, ni système de fichiers, ni processus.
import { Channel, invoke } from "@tauri-apps/api/core";

import type { DiffLine } from "../bindings/DiffLine";
import type { ExportOptions } from "../bindings/ExportOptions";
import type { ExportReport } from "../bindings/ExportReport";
import type { GenerateOptions } from "../bindings/GenerateOptions";
import type { GenerationResult } from "../bindings/GenerationResult";
import type { GenEvent } from "../bindings/GenEvent";
import type { ImportedModel } from "../bindings/ImportedModel";
import type { ImportedSkill } from "../bindings/ImportedSkill";
import type { Issue } from "../bindings/Issue";
import type { ModelStatus } from "../bindings/ModelStatus";
import type { OnlineSkill } from "../bindings/OnlineSkill";
import type { Project } from "../bindings/Project";
import type { ProjectInput } from "../bindings/ProjectInput";
import type { ProjectQuery } from "../bindings/ProjectQuery";
import type { ProjectStatus } from "../bindings/ProjectStatus";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import type { RepoProfile } from "../bindings/RepoProfile";
import type { Settings } from "../bindings/Settings";
import type { Skill } from "../bindings/Skill";
import type { SkillFile } from "../bindings/SkillFile";
import type { SkillMeta } from "../bindings/SkillMeta";
import type { SkillOrigin } from "../bindings/SkillOrigin";
import type { SkillProposal } from "../bindings/SkillProposal";
import type { VersionMeta } from "../bindings/VersionMeta";
import type {
  CmdError,
  DownloadProgress,
  ExportKind,
  JournalEntry,
  ProjectDetail,
  RuntimeStatus,
  SystemInfo,
  TextEvent,
} from "./types";

export function isCmdError(e: unknown): e is CmdError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

function channel<T>(onMessage: (event: T) => void): Channel<T> {
  const ch = new Channel<T>();
  ch.onmessage = onMessage;
  return ch;
}

export const api = {
  // Projets
  listProjects: (query: ProjectQuery) => invoke<ProjectSummary[]>("list_projects", { query }),
  getProject: (id: string) => invoke<ProjectDetail>("get_project", { id }),
  createProject: (input: ProjectInput) => invoke<Project>("create_project", { input }),
  updateProject: (id: string, input: ProjectInput) => invoke<Project>("update_project", { id, input }),
  setProjectStatus: (id: string, status: ProjectStatus) => invoke<Project>("set_project_status", { id, status }),
  deleteProject: (id: string) => invoke<boolean>("delete_project", { id }),
  analyzeRepo: (id: string) => invoke<RepoProfile | null>("analyze_repo", { id }),
  pickFolder: () => invoke<string | null>("pick_folder"),
  importBrandFiles: (id: string) => invoke<ProjectDetail>("import_brand_files", { id }),
  removeBrandFile: (id: string, name: string) => invoke<ProjectDetail>("remove_brand_file", { id, name }),

  // AGENTS.md
  generateAgents: (id: string, options: GenerateOptions, onEvent: (e: GenEvent) => void) =>
    invoke<GenerationResult>("generate_agents", { id, options, onEvent: channel(onEvent) }),
  cancelOperation: (op: string) => invoke<void>("cancel_operation", { op }),
  saveAgents: (id: string, content: string, note: string | null) =>
    invoke<VersionMeta>("save_agents", { id, content, note }),
  readVersion: (id: string, number: number) => invoke<string>("read_version", { id, number }),
  restoreVersion: (id: string, number: number) => invoke<VersionMeta>("restore_version", { id, number }),
  diffVersions: (id: string, from: number, to: number | null) =>
    invoke<DiffLine[]>("diff_versions", { id, from, to }),
  checkAgents: (id: string) => invoke<Issue[]>("check_agents", { id }),
  exportProject: (id: string, kind: ExportKind, options: ExportOptions) =>
    invoke<ExportReport | null>("export_project", { id, kind, options }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),

  // Skills
  listSkills: () => invoke<SkillMeta[]>("list_skills"),
  getSkill: (slug: string) => invoke<Skill>("get_skill", { slug }),
  blankSkillTemplate: (name: string, description: string) =>
    invoke<string>("blank_skill_template", { name, description }),
  saveNewSkill: (content: string, files: SkillFile[], origin: SkillOrigin) =>
    invoke<Skill>("save_new_skill", { content, files, origin }),
  updateSkill: (slug: string, content: string, files: SkillFile[]) =>
    invoke<Skill>("update_skill", { slug, content, files }),
  deleteSkill: (slug: string) => invoke<boolean>("delete_skill", { slug }),
  proposeNewSkill: (request: string, projectId: string | null, onEvent: (e: TextEvent) => void) =>
    invoke<SkillProposal>("propose_new_skill", { request, projectId, onEvent: channel(onEvent) }),
  listProjectSkills: (id: string) => invoke<Skill[]>("list_project_skills", { id }),
  attachSkill: (id: string, slug: string) => invoke<Skill>("attach_skill", { id, slug }),
  detachSkill: (id: string, slug: string) => invoke<void>("detach_skill", { id, slug }),
  updateProjectSkill: (id: string, slug: string, content: string, files: SkillFile[], adapted: boolean) =>
    invoke<Skill>("update_project_skill", { id, slug, content, files, adapted }),
  adaptProjectSkill: (id: string, slug: string, instruction: string | null, onEvent: (e: TextEvent) => void) =>
    invoke<SkillProposal>("adapt_project_skill", { id, slug, instruction, onEvent: channel(onEvent) }),
  searchOnlineSkills: (query: string) => invoke<OnlineSkill[]>("search_online_skills", { query }),
  importOnlineSkill: (found: OnlineSkill) => invoke<ImportedSkill>("import_online_skill", { found }),

  // Réglages, modèles, moteur
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  systemInfo: () => invoke<SystemInfo>("system_info"),
  listModels: () => invoke<ModelStatus[]>("list_models"),
  startModelDownload: (modelId: string, onProgress: (p: DownloadProgress) => void) =>
    invoke<string>("start_model_download", { modelId, onProgress: channel(onProgress) }),
  cancelModelDownload: (modelId: string) => invoke<void>("cancel_model_download", { modelId }),
  importModelFile: () => invoke<ImportedModel | null>("import_model_file"),
  forgetModel: (modelId: string) => invoke<void>("forget_model", { modelId }),
  runtimeStatus: () => invoke<RuntimeStatus>("runtime_status"),
  runtimeLog: () => invoke<string[]>("runtime_log"),
  startRuntime: () => invoke<RuntimeStatus>("start_runtime"),
  stopRuntime: () => invoke<RuntimeStatus>("stop_runtime"),
  readNetworkLog: (limit: number) => invoke<JournalEntry[]>("read_network_log", { limit }),
  openExternalLink: (url: string) => invoke<boolean>("open_external_link", { url }),
  revealDataDir: () => invoke<void>("reveal_data_dir"),
};
