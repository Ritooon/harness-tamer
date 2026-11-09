import type { ProjectSummary } from "../bindings/ProjectSummary";
import type { Settings } from "../bindings/Settings";

export function settings(patch: Partial<Settings> = {}): Settings {
  return {
    schema_version: 1,
    ui_language: "fr",
    default_output_language: "fr",
    network_policy: "ask",
    active_model: null,
    models_dir: null,
    default_export_dir: null,
    runtime: { backend: "embedded", context_size: 32768, gpu_layers: null, cpu_moe_layers: null, ollama_model: null },
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
