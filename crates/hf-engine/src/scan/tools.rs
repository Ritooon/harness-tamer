//! Outils de qualité configurés, workflows de CI et indentation EditorConfig.

use std::path::Path;

use super::{CiWorkflow, Reader, Tool, ToolKind};

/// Fichier de configuration (préfixe du nom) → outil.
const TOOL_FILES: &[(&str, &str, ToolKind)] = &[
    (".eslintrc", "ESLint", ToolKind::Lint),
    ("eslint.config.", "ESLint", ToolKind::Lint),
    (".prettierrc", "Prettier", ToolKind::Format),
    ("prettier.config.", "Prettier", ToolKind::Format),
    ("biome.json", "Biome", ToolKind::Lint),
    (".stylelintrc", "Stylelint", ToolKind::Lint),
    ("tsconfig.json", "TypeScript", ToolKind::TypeCheck),
    ("jest.config.", "Jest", ToolKind::Test),
    ("vitest.config.", "Vitest", ToolKind::Test),
    ("playwright.config.", "Playwright", ToolKind::Test),
    ("cypress.config.", "Cypress", ToolKind::Test),
    ("ruff.toml", "Ruff", ToolKind::Lint),
    (".ruff.toml", "Ruff", ToolKind::Lint),
    (".flake8", "Flake8", ToolKind::Lint),
    ("mypy.ini", "mypy", ToolKind::TypeCheck),
    ("pytest.ini", "pytest", ToolKind::Test),
    ("tox.ini", "tox", ToolKind::Test),
    ("rustfmt.toml", "rustfmt", ToolKind::Format),
    (".rustfmt.toml", "rustfmt", ToolKind::Format),
    ("clippy.toml", "Clippy", ToolKind::Lint),
    ("deny.toml", "cargo-deny", ToolKind::Lint),
    (".golangci.", "golangci-lint", ToolKind::Lint),
    (".rubocop.yml", "RuboCop", ToolKind::Lint),
    ("phpstan.neon", "PHPStan", ToolKind::Lint),
    ("phpunit.xml", "PHPUnit", ToolKind::Test),
    ("checkstyle.xml", "Checkstyle", ToolKind::Lint),
    ("detekt.yml", "detekt", ToolKind::Lint),
    (".swiftlint.yml", "SwiftLint", ToolKind::Lint),
    ("analysis_options.yaml", "Dart analyzer", ToolKind::Lint),
    (".pre-commit-config.yaml", "pre-commit", ToolKind::Hooks),
    ("lefthook.yml", "Lefthook", ToolKind::Hooks),
    (".editorconfig", "EditorConfig", ToolKind::Editor),
    ("Dockerfile", "Docker", ToolKind::Container),
    ("docker-compose.", "Docker Compose", ToolKind::Container),
    ("compose.yaml", "Docker Compose", ToolKind::Container),
    ("vite.config.", "Vite", ToolKind::Build),
    ("webpack.config.", "webpack", ToolKind::Build),
    ("turbo.json", "Turborepo", ToolKind::Build),
    ("nx.json", "Nx", ToolKind::Build),
];

/// Outils détectés à la racine (ou à un niveau, pour les monorepos).
pub(super) fn detect(paths: &[String], reader: &Reader<'_>) -> Vec<Tool> {
    let mut tools: Vec<Tool> = Vec::new();
    for path in paths.iter().filter(|p| p.split('/').count() <= 2) {
        let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("");
        for (prefix, tool, kind) in TOOL_FILES {
            if name.starts_with(prefix) && !tools.iter().any(|t| t.name == *tool) {
                tools.push(Tool { name: (*tool).into(), kind: *kind, config: path.clone() });
            }
        }
    }
    if paths.iter().any(|p| p.starts_with(".husky/")) {
        tools.push(Tool { name: "Husky".into(), kind: ToolKind::Hooks, config: ".husky/".into() });
    }
    // Outils Python déclarés dans pyproject.toml.
    if let Some(pyproject) = reader.read("pyproject.toml") {
        for (section, tool, kind) in [
            ("[tool.ruff", "Ruff", ToolKind::Lint),
            ("[tool.black", "Black", ToolKind::Format),
            ("[tool.mypy", "mypy", ToolKind::TypeCheck),
            ("[tool.pytest", "pytest", ToolKind::Test),
        ] {
            if pyproject.contains(section) && !tools.iter().any(|t| t.name == tool) {
                tools.push(Tool { name: tool.into(), kind, config: "pyproject.toml".into() });
            }
        }
    }
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

/// Workflows de CI et commandes qu'ils exécutent (GitHub Actions, GitLab CI).
pub(super) fn ci_workflows(paths: &[String], reader: &Reader<'_>) -> Vec<CiWorkflow> {
    let mut out = Vec::new();
    for path in paths {
        let provider = if path.starts_with(".github/workflows/") && (path.ends_with(".yml") || path.ends_with(".yaml"))
        {
            "GitHub Actions"
        } else if path == ".gitlab-ci.yml" {
            "GitLab CI"
        } else {
            continue;
        };
        let Some(text) = reader.read(path) else { continue };
        let commands =
            if provider == "GitHub Actions" { github_run_commands(&text) } else { gitlab_script_commands(&text) };
        out.push(CiWorkflow { path: path.clone(), provider: provider.into(), commands });
    }
    out
}

/// Extrait les commandes des clés `run:` (une ligne ou bloc `|`).
fn github_run_commands(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = yaml.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start().trim_start_matches("- ");
        if let Some(value) = trimmed.strip_prefix("run:") {
            let value = value.trim();
            if value.starts_with('|') || value.starts_with('>') {
                let indent = line.len() - line.trim_start().len();
                i += 1;
                while i < lines.len() {
                    let next = lines[i];
                    let next_indent = next.len() - next.trim_start().len();
                    if !next.trim().is_empty() && next_indent <= indent {
                        break;
                    }
                    push_command(&mut out, next.trim());
                    i += 1;
                }
                continue;
            }
            push_command(&mut out, value.trim_matches('"').trim_matches('\''));
        }
        i += 1;
    }
    out
}

/// Extrait les éléments des listes `script:` d'un `.gitlab-ci.yml`.
fn gitlab_script_commands(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    let mut indent = 0;
    for line in yaml.lines() {
        let trimmed = line.trim_start();
        let current = line.len() - trimmed.len();
        if trimmed.starts_with("script:") || trimmed.starts_with("before_script:") {
            inside = true;
            indent = current;
            continue;
        }
        if inside {
            if let Some(item) = trimmed.strip_prefix("- ")
                && current >= indent
            {
                push_command(&mut out, item.trim_matches('"').trim_matches('\''));
            } else if !trimmed.is_empty() && current <= indent {
                inside = false;
            }
        }
    }
    out
}

fn push_command(out: &mut Vec<String>, command: &str) {
    let command = command.trim();
    if command.is_empty() || command.starts_with('#') || command.contains("${{") {
        return;
    }
    if !out.iter().any(|c| c == command) {
        out.push(command.to_owned());
    }
}

/// Indentation déclarée pour tous les fichiers (`[*]`) dans `.editorconfig`.
pub(super) fn editorconfig_indent(paths: &[String], reader: &Reader<'_>) -> Option<String> {
    if !paths.iter().any(|p| p == ".editorconfig") {
        return None;
    }
    let text = reader.read(".editorconfig")?;
    let mut in_star = false;
    let (mut style, mut size) = (None, None);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_star = line == "[*]";
            continue;
        }
        if !in_star {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "indent_style" => style = Some(v.trim().to_owned()),
                "indent_size" => size = Some(v.trim().to_owned()),
                _ => {}
            }
        }
    }
    match (style.as_deref(), size) {
        (Some("tab"), _) => Some("tab".into()),
        (_, Some(n)) => Some(format!("{n} spaces")),
        (Some(s), None) => Some(s.to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_run_steps_single_and_multiline() {
        let yaml = "jobs:\n  t:\n    steps:\n      - run: npm ci\n      - name: Test\n        run: |\n          npm run lint\n          npm test\n      - run: echo ${{ secrets.X }}\n";
        assert_eq!(github_run_commands(yaml), vec!["npm ci", "npm run lint", "npm test"]);
    }

    #[test]
    fn gitlab_scripts() {
        let yaml = "test:\n  script:\n    - cargo test\n    - cargo clippy\n  stage: test\n";
        assert_eq!(gitlab_script_commands(yaml), vec!["cargo test", "cargo clippy"]);
    }
}
