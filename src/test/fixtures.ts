import type { Project } from "../bindings/Project";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import type { Settings } from "../bindings/Settings";
import type { VersionMeta } from "../bindings/VersionMeta";
import type { ProjectDetail } from "../lib/types";

export function settings(patch: Partial<Settings> = {}): Settings {
  return {
    schema_version: 1,
    ui_language: "fr",
    default_output_language: "fr",
    network_policy: "ask",
    active_model: null,
    models_dir: null,
    default_export_dir: null,
    runtime: { backend: "embedded", context_size: 32768, gpu_layers: null, cpu_moe_layers: null, ollama_model: null, server_path: null },
    skill_sources: [],
    ...patch,
  };
}

export function summary(patch: Partial<ProjectSummary> = {}): ProjectSummary {
  return {
    id: "p1",
    name: "Boutique",
    description: "Site marchand",
    status: "active",
    platform: "web",
    imposed_tech: ["React", "Rust"],
    has_agents: false,
    created_at: "2026-10-01T10:00:00Z",
    updated_at: "2026-11-01T10:00:00Z",
    ...patch,
  };
}

export function version(patch: Partial<VersionMeta> = {}): VersionMeta {
  return {
    number: 1,
    created_at: "2026-11-01T10:00:00Z",
    origin: "generation",
    model: "Qwen",
    sections: [],
    restored_from: null,
    note: null,
    ...patch,
  };
}

export function project(patch: Partial<Project> = {}): Project {
  return {
    schema_version: 1,
    id: "p1",
    name: "Boutique",
    description: "Site marchand",
    status: "active",
    platform: "web",
    target_os: ["browser"],
    imposed_tech: ["React", "Rust"],
    existing_code: { enabled: false, repo_path: null },
    brand: { enabled: false, files: [] },
    constraints: "",
    output_language: "fr",
    created_at: "2026-10-01T10:00:00Z",
    updated_at: "2026-11-01T10:00:00Z",
    ...patch,
  };
}

export function detail(patch: Partial<ProjectDetail> = {}): ProjectDetail {
  return { project: project(), agents: null, versions: [], skills: [], repo_profile: null, brand_kit: null, ...patch };
}
