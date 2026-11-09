//! Manifestes de dépendances et commandes de build/test/lint qui en découlent.

use std::collections::BTreeSet;
use std::path::Path;

use regex::Regex;
use serde_json::Value as Json;

use super::{CiWorkflow, Manifest, ManifestKind, Reader, RepoCommand};

/// Profondeur maximale (en segments de chemin) d'un manifeste pris en compte.
const MAX_MANIFEST_DEPTH: usize = 3;

fn kind_of(path: &str) -> Option<ManifestKind> {
    let name = Path::new(path).file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    Some(match lower.as_str() {
        "package.json" => ManifestKind::Npm,
        "cargo.toml" => ManifestKind::Cargo,
        "pyproject.toml" => ManifestKind::PyProject,
        "go.mod" => ManifestKind::GoMod,
        "pom.xml" => ManifestKind::Maven,
        "build.gradle" | "build.gradle.kts" => ManifestKind::Gradle,
        "composer.json" => ManifestKind::Composer,
        "gemfile" => ManifestKind::Gemfile,
        "pubspec.yaml" => ManifestKind::Pubspec,
        "package.swift" => ManifestKind::SwiftPackage,
        "cmakelists.txt" => ManifestKind::CMake,
        "makefile" | "gnumakefile" => ManifestKind::Makefile,
        _ if lower.starts_with("requirements") && lower.ends_with(".txt") => ManifestKind::Requirements,
        _ if lower.ends_with(".csproj") || lower.ends_with(".fsproj") => ManifestKind::DotNet,
        _ => return None,
    })
}

/// Lit tous les manifestes reconnus, jusqu'à trois niveaux de profondeur.
pub(super) fn parse_all(paths: &[String], reader: &Reader<'_>) -> Vec<Manifest> {
    paths
        .iter()
        .filter(|p| p.split('/').count() <= MAX_MANIFEST_DEPTH)
        .filter_map(|p| {
            let kind = kind_of(p)?;
            let text = reader.read(p)?;
            Some(parse(p, kind, &text))
        })
        .collect()
}

fn parse(path: &str, kind: ManifestKind, text: &str) -> Manifest {
    let mut m = Manifest { path: path.to_owned(), kind, name: None, dependencies: vec![], dev_dependencies: vec![] };
    match kind {
        ManifestKind::Npm | ManifestKind::Composer => {
            if let Ok(json) = serde_json::from_str::<Json>(text) {
                m.name = json["name"].as_str().map(str::to_owned);
                let (deps, dev) = if kind == ManifestKind::Npm {
                    ("dependencies", "devDependencies")
                } else {
                    ("require", "require-dev")
                };
                m.dependencies = json_keys(&json[deps]);
                m.dev_dependencies = json_keys(&json[dev]);
            }
        }
        ManifestKind::Cargo => {
            if let Ok(toml) = text.parse::<toml::Table>() {
                m.name = toml.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(str::to_owned);
                m.dependencies = toml_keys(toml.get("dependencies"));
                m.dev_dependencies = toml_keys(toml.get("dev-dependencies"));
            }
        }
        ManifestKind::PyProject => {
            if let Ok(toml) = text.parse::<toml::Table>() {
                let project = toml.get("project");
                m.name = project.and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(str::to_owned);
                let mut deps = toml_requirement_list(project.and_then(|p| p.get("dependencies")));
                let poetry = toml.get("tool").and_then(|t| t.get("poetry"));
                deps.extend(
                    toml_keys(poetry.and_then(|p| p.get("dependencies"))).into_iter().filter(|d| d != "python"),
                );
                m.dependencies = deps;
                let mut dev = Vec::new();
                for table in [project.and_then(|p| p.get("optional-dependencies")), toml.get("dependency-groups")]
                    .into_iter()
                    .flatten()
                {
                    if let Some(groups) = table.as_table() {
                        for list in groups.values() {
                            dev.extend(toml_requirement_list(Some(list)));
                        }
                    }
                }
                m.dev_dependencies = dev;
            }
        }
        ManifestKind::Requirements => {
            m.dependencies = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('-'))
                .map(requirement_name)
                .collect();
        }
        ManifestKind::GoMod => {
            let re = Regex::new(r"(?m)^\s*(?:require\s+)?([a-zA-Z0-9.\-]+\.[a-z]{2,}/[^\s]+)\s+v[0-9]").unwrap();
            m.name = text.lines().find_map(|l| l.strip_prefix("module ")).map(|s| s.trim().to_owned());
            m.dependencies = re.captures_iter(text).map(|c| c[1].to_owned()).collect();
        }
        ManifestKind::Maven => {
            let artifact = Regex::new(r"<artifactId>\s*([^<\s]+)\s*</artifactId>").unwrap();
            let mut ids = artifact.captures_iter(text).map(|c| c[1].to_owned());
            m.name = ids.next();
            m.dependencies = ids.collect();
        }
        ManifestKind::Gradle => {
            let re = Regex::new(
                r#"(?m)^\s*(implementation|api|compileOnly|runtimeOnly|testImplementation|androidTestImplementation)\s*\(?\s*["']([^"':]+:[^"':]+)"#,
            )
            .unwrap();
            for c in re.captures_iter(text) {
                if c[1].starts_with("test") || c[1].starts_with("androidTest") {
                    m.dev_dependencies.push(c[2].to_owned());
                } else {
                    m.dependencies.push(c[2].to_owned());
                }
            }
        }
        ManifestKind::Gemfile => {
            let re = Regex::new(r#"(?m)^\s*gem\s+["']([^"']+)["']"#).unwrap();
            m.dependencies = re.captures_iter(text).map(|c| c[1].to_owned()).collect();
        }
        ManifestKind::DotNet => {
            let re = Regex::new(r#"<PackageReference\s+Include="([^"]+)""#).unwrap();
            m.dependencies = re.captures_iter(text).map(|c| c[1].to_owned()).collect();
            m.name = Path::new(&m.path).file_stem().and_then(|s| s.to_str()).map(str::to_owned);
        }
        ManifestKind::Pubspec => {
            m.name = text.lines().find_map(|l| l.strip_prefix("name:")).map(|s| s.trim().to_owned());
            m.dependencies = yaml_block_keys(text, "dependencies");
            m.dev_dependencies = yaml_block_keys(text, "dev_dependencies");
        }
        ManifestKind::CMake => {
            let re = Regex::new(r"(?i)project\s*\(\s*([A-Za-z0-9_\-]+)").unwrap();
            m.name = re.captures(text).map(|c| c[1].to_owned());
        }
        ManifestKind::SwiftPackage => {
            let re = Regex::new(r#"name:\s*"([^"]+)""#).unwrap();
            m.name = re.captures(text).map(|c| c[1].to_owned());
        }
        ManifestKind::Makefile => {}
    }
    m.dependencies.sort();
    m.dependencies.dedup();
    m.dev_dependencies.sort();
    m.dev_dependencies.dedup();
    m
}

fn json_keys(v: &Json) -> Vec<String> {
    v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
}

fn toml_keys(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|t| t.as_table()).map(|t| t.keys().cloned().collect()).unwrap_or_default()
}

fn toml_requirement_list(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(requirement_name).collect())
        .unwrap_or_default()
}

/// `requests[socks]>=2.0 ; python_version>"3"` → `requests`.
fn requirement_name(spec: &str) -> String {
    spec.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .next()
        .unwrap_or("")
        .to_owned()
}

/// Clés d'un bloc YAML de premier niveau (`dependencies:` suivi de lignes indentées).
fn yaml_block_keys(text: &str, block: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim_end() == format!("{block}:") {
            inside = true;
            continue;
        }
        if inside {
            if !line.starts_with(' ') && !line.trim().is_empty() {
                break;
            }
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if indent == 2
                && let Some((key, _)) = trimmed.split_once(':')
            {
                keys.push(key.trim().to_owned());
            }
        }
    }
    keys
}

fn has(paths: &[String], name: &str) -> bool {
    paths.iter().any(|p| p == name)
}

/// Gestionnaires de paquets, déduits des fichiers de verrouillage.
pub(super) fn package_managers(paths: &[String], manifests: &[Manifest]) -> Vec<String> {
    let mut out = BTreeSet::new();
    let kinds: Vec<ManifestKind> = manifests.iter().map(|m| m.kind).collect();
    if kinds.contains(&ManifestKind::Npm) {
        out.insert(js_package_manager(paths).to_owned());
    }
    if kinds.contains(&ManifestKind::Cargo) {
        out.insert("cargo".into());
    }
    if kinds.contains(&ManifestKind::PyProject) || kinds.contains(&ManifestKind::Requirements) {
        let runner = python_runner(paths).and_then(|r| r.split_whitespace().next()).unwrap_or("pip");
        out.insert(runner.to_owned());
    }
    if kinds.contains(&ManifestKind::GoMod) {
        out.insert("go".into());
    }
    if kinds.contains(&ManifestKind::Maven) {
        out.insert("maven".into());
    }
    if kinds.contains(&ManifestKind::Gradle) {
        out.insert("gradle".into());
    }
    if kinds.contains(&ManifestKind::Composer) {
        out.insert("composer".into());
    }
    if kinds.contains(&ManifestKind::Gemfile) {
        out.insert("bundler".into());
    }
    if kinds.contains(&ManifestKind::DotNet) {
        out.insert("dotnet".into());
    }
    if kinds.contains(&ManifestKind::Pubspec) {
        let mobile = paths.iter().any(|p| p.starts_with("android/") || p.starts_with("ios/"));
        out.insert(if mobile { "flutter" } else { "dart" }.to_owned());
    }
    out.into_iter().collect()
}

fn js_package_manager(paths: &[String]) -> &'static str {
    if has(paths, "pnpm-lock.yaml") {
        "pnpm"
    } else if has(paths, "yarn.lock") {
        "yarn"
    } else if has(paths, "bun.lockb") || has(paths, "bun.lock") {
        "bun"
    } else {
        "npm"
    }
}

/// Préfixe d'exécution Python (`uv run `, `poetry run `) ou `None` pour pip.
fn python_runner(paths: &[String]) -> Option<&'static str> {
    if has(paths, "uv.lock") {
        Some("uv run ")
    } else if has(paths, "poetry.lock") {
        Some("poetry run ")
    } else if has(paths, "Pipfile") {
        Some("pipenv run ")
    } else {
        None
    }
}

/// Rôle d'un script npm/composer d'après son nom.
fn script_role(name: &str) -> Option<&'static str> {
    let base = name.split(':').next().unwrap_or(name);
    Some(match base {
        "build" | "compile" | "bundle" => "build",
        "test" | "tests" | "e2e" | "coverage" => "test",
        "lint" | "eslint" | "stylelint" => "lint",
        "format" | "fmt" | "prettier" => "format",
        "typecheck" | "type-check" | "types" | "tsc" | "check" => "typecheck",
        "dev" | "start" | "serve" | "preview" => "dev",
        _ => return None,
    })
}

/// Commandes de build, test, lint… dédoublonnées, dans un ordre stable.
pub(super) fn commands(
    paths: &[String],
    manifests: &[Manifest],
    package_managers: &[String],
    reader: &Reader<'_>,
    ci: &[CiWorkflow],
) -> Vec<RepoCommand> {
    let mut out: Vec<RepoCommand> = Vec::new();
    let mut push = |role: &str, command: String, source: &str| {
        if !out.iter().any(|c| normalize_command(&c.command) == normalize_command(&command)) {
            out.push(RepoCommand { role: role.into(), command, source: source.into() });
        }
    };
    let deps_of = |kind: ManifestKind| -> BTreeSet<String> {
        manifests
            .iter()
            .filter(|m| m.kind == kind)
            .flat_map(|m| m.dependencies.iter().chain(&m.dev_dependencies))
            .map(|d| d.to_ascii_lowercase())
            .collect()
    };

    // JavaScript / TypeScript : scripts du package.json racine.
    if has(paths, "package.json")
        && let Some(json) = reader.read("package.json").and_then(|t| serde_json::from_str::<Json>(&t).ok())
        && let Some(scripts) = json["scripts"].as_object()
    {
        let pm = js_package_manager(paths);
        for name in scripts.keys() {
            if let Some(role) = script_role(name) {
                push(role, format!("{pm} run {name}"), "package.json");
            }
        }
    }

    if package_managers.iter().any(|p| p == "cargo") {
        push("build", "cargo build".into(), "convention");
        push("test", "cargo test".into(), "convention");
        push("lint", "cargo clippy --all-targets".into(), "convention");
        push("format", "cargo fmt --check".into(), "convention");
    }

    if manifests.iter().any(|m| matches!(m.kind, ManifestKind::PyProject | ManifestKind::Requirements)) {
        let deps = deps_of(ManifestKind::PyProject)
            .into_iter()
            .chain(deps_of(ManifestKind::Requirements))
            .collect::<BTreeSet<_>>();
        let pyproject = reader.read("pyproject.toml").unwrap_or_default();
        let run = python_runner(paths).unwrap_or("");
        if deps.contains("pytest") || pyproject.contains("[tool.pytest") || has(paths, "pytest.ini") {
            push("test", format!("{run}pytest"), "convention");
        }
        if deps.contains("ruff")
            || pyproject.contains("[tool.ruff")
            || has(paths, "ruff.toml")
            || has(paths, ".ruff.toml")
        {
            push("lint", format!("{run}ruff check ."), "convention");
            push("format", format!("{run}ruff format ."), "convention");
        } else if deps.contains("black") || pyproject.contains("[tool.black") {
            push("format", format!("{run}black ."), "convention");
        }
        if deps.contains("mypy") || pyproject.contains("[tool.mypy") || has(paths, "mypy.ini") {
            push("typecheck", format!("{run}mypy ."), "convention");
        }
    }

    if package_managers.iter().any(|p| p == "go") {
        push("build", "go build ./...".into(), "convention");
        push("test", "go test ./...".into(), "convention");
        push("lint", "go vet ./...".into(), "convention");
        if paths.iter().any(|p| p.starts_with(".golangci.")) {
            push("lint", "golangci-lint run".into(), ".golangci");
        }
    }

    if package_managers.iter().any(|p| p == "maven") {
        push("build", "mvn -B package".into(), "convention");
        push("test", "mvn -B test".into(), "convention");
    }
    if package_managers.iter().any(|p| p == "gradle") {
        let gradle = if has(paths, "gradlew") { "./gradlew" } else { "gradle" };
        push("build", format!("{gradle} build"), "convention");
        push("test", format!("{gradle} test"), "convention");
    }

    if has(paths, "composer.json")
        && let Some(json) = reader.read("composer.json").and_then(|t| serde_json::from_str::<Json>(&t).ok())
    {
        if let Some(scripts) = json["scripts"].as_object() {
            for name in scripts.keys() {
                if let Some(role) = script_role(name) {
                    push(role, format!("composer run {name}"), "composer.json");
                }
            }
        }
        if deps_of(ManifestKind::Composer).contains("phpunit/phpunit") {
            push("test", "vendor/bin/phpunit".into(), "convention");
        }
    }

    if package_managers.iter().any(|p| p == "dotnet") {
        push("build", "dotnet build".into(), "convention");
        push("test", "dotnet test".into(), "convention");
        push("format", "dotnet format --verify-no-changes".into(), "convention");
    }

    if let Some(tool) = package_managers.iter().find(|p| *p == "flutter" || *p == "dart") {
        push("test", format!("{tool} test"), "convention");
        push("lint", format!("{tool} analyze"), "convention");
        push("format", "dart format .".into(), "convention");
    }

    if package_managers.iter().any(|p| p == "bundler") {
        let deps = deps_of(ManifestKind::Gemfile);
        if deps.contains("rspec") || deps.contains("rspec-rails") {
            push("test", "bundle exec rspec".into(), "convention");
        }
        if deps.contains("rubocop") {
            push("lint", "bundle exec rubocop".into(), "convention");
        }
    }

    // Cibles du Makefile racine.
    if let Some(makefile) = ["Makefile", "GNUmakefile", "makefile"].iter().find(|m| has(paths, m))
        && let Some(text) = reader.read(makefile)
    {
        let re = Regex::new(r"(?m)^([A-Za-z0-9][A-Za-z0-9_.\-]*)\s*:([^=]|$)").unwrap();
        for c in re.captures_iter(&text) {
            let target = &c[1];
            let role = match target {
                "build" | "all" => "build",
                "test" | "tests" | "check" => "test",
                "lint" => "lint",
                "fmt" | "format" => "format",
                "run" | "dev" | "serve" => "dev",
                _ => continue,
            };
            push(role, format!("make {target}"), "Makefile");
        }
    }

    // Commandes exécutées par la CI et pas encore listées.
    for workflow in ci {
        for command in &workflow.commands {
            let lower = command.to_ascii_lowercase();
            let role = if lower.contains("test") {
                "test"
            } else if lower.contains("lint") || lower.contains("clippy") || lower.contains("vet") {
                "lint"
            } else if lower.contains("fmt") || lower.contains("format") || lower.contains("prettier") {
                "format"
            } else if lower.contains("build") {
                "build"
            } else {
                continue;
            };
            push(role, command.clone(), "CI");
        }
    }
    out
}

/// Forme canonique d'une commande, pour comparer `npm test` et `npm run test`.
pub fn normalize_command(command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().collect();
    let Some((&first, rest)) = words.split_first() else { return String::new() };
    let first = first.to_ascii_lowercase();
    let mut rest: Vec<String> = rest.iter().map(|w| w.to_string()).collect();
    if matches!(first.as_str(), "npm" | "pnpm" | "yarn" | "bun") && !rest.is_empty() {
        let sub = rest[0].clone();
        if sub == "run" || sub == "run-script" {
            rest[0] = "run".into();
        } else if first == "npm" {
            // npm n'accepte sans `run` que quelques scripts réservés.
            if matches!(sub.as_str(), "test" | "t" | "start" | "stop" | "restart") {
                rest[0] = if sub == "t" { "test".into() } else { sub };
                rest.insert(0, "run".into());
            }
        } else if !matches!(
            sub.as_str(),
            "install" | "i" | "add" | "remove" | "exec" | "dlx" | "x" | "ci" | "create" | "init" | "why"
        ) {
            // pnpm, yarn et bun lancent un script sans `run`.
            rest.insert(0, "run".into());
        }
    }
    std::iter::once(first).chain(rest).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_js_commands_normalise_to_the_same_form() {
        assert_eq!(normalize_command("npm test"), "npm run test");
        assert_eq!(normalize_command("npm  run   test"), "npm run test");
        assert_eq!(normalize_command("pnpm lint"), "pnpm run lint");
        assert_eq!(normalize_command("yarn run build"), "yarn run build");
        assert_eq!(normalize_command("pnpm install"), "pnpm install");
        assert_eq!(normalize_command("cargo test"), "cargo test");
    }

    #[test]
    fn requirement_names_drop_versions_and_extras() {
        assert_eq!(requirement_name("requests[socks]>=2.0 ; python_version>'3'"), "requests");
        assert_eq!(requirement_name("Django==5.0"), "Django");
    }

    #[test]
    fn yaml_blocks_list_their_keys() {
        let text =
            "name: app\ndependencies:\n  flutter:\n    sdk: flutter\n  http: ^1.0\ndev_dependencies:\n  test: any\n";
        assert_eq!(yaml_block_keys(text, "dependencies"), vec!["flutter", "http"]);
        assert_eq!(yaml_block_keys(text, "dev_dependencies"), vec!["test"]);
    }
}
