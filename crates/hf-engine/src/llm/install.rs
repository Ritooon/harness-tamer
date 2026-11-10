//! Installation de `llama-server` depuis l'application, sur demande.
//!
//! Les installeurs livrent déjà le moteur. Ce module sert quand il manque
//! (build de développement, paquet minimal) : l'utilisateur clique sur
//! « Installer le moteur », le dialogue de consentement s'ouvre (action
//! `runtime_download`), puis l'archive officielle de llama.cpp est
//! téléchargée, vérifiée et décompressée dans `<données>/runtimes/llama-cpp/`.
//!
//! Rien n'est exécuté ni installé sans empreinte SHA-256 vérifiée : celle du
//! fichier `resources/catalog/llama.json` si elle est renseignée, sinon celle
//! que GitHub publie pour l'archive. Sans l'une ni l'autre, l'installation
//! est refusée.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use super::runtime::SERVER_BINARY;
use crate::error::{EngineError, Result};
use crate::model::Lang;

const BUILTIN: &str = include_str!("../../../../resources/catalog/llama.json");

/// Hôtes vers lesquels GitHub redirige le téléchargement d'une archive.
const DOWNLOAD_HOSTS: [&str; 3] =
    ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

/// Version de llama.cpp fixée pour l'application et pour les builds.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeLock {
    pub repo: String,
    pub tag: String,
    pub targets: BTreeMap<String, Vec<LockedArchive>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LockedArchive {
    /// Motif du nom d'archive, pour la retrouver dans la version publiée.
    pub pattern: String,
    pub file: Option<String>,
    pub sha256: Option<String>,
}

impl RuntimeLock {
    pub fn builtin() -> Self {
        serde_json::from_str(BUILTIN).expect("resources/catalog/llama.json est valide (vérifié par les tests)")
    }
}

/// Clé de la plateforme courante dans le verrou (`macos-arm64/metal`…), si
/// llama.cpp publie une archive pour elle.
pub fn platform_key(os: &str, arch: &str) -> Option<String> {
    let os = match os {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "win",
        _ => return None,
    };
    let arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    // Metal sur Apple Silicon ; la build CPU ailleurs, qui fonctionne partout.
    let variant = if (os, arch) == ("macos", "arm64") { "metal" } else { "cpu" };
    Some(format!("{os}-{arch}/{variant}"))
}

#[derive(Debug, Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: Url,
    #[serde(default)]
    size: u64,
    /// `sha256:<hex>`, publié par GitHub pour les archives récentes.
    #[serde(default)]
    digest: Option<String>,
}

/// Archive à télécharger, avec l'empreinte qui permettra de la vérifier.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Planned {
    file: String,
    url: Url,
    sha256: String,
}

fn release_api(lock: &RuntimeLock) -> Result<Url> {
    Url::parse(&format!("https://api.github.com/repos/{}/releases/tags/{}", lock.repo, lock.tag))
        .map_err(|e| EngineError::RuntimeInstall(e.to_string()))
}

fn direct_url(lock: &RuntimeLock, file: &str) -> Result<Url> {
    Url::parse(&format!("https://github.com/{}/releases/download/{}/{file}", lock.repo, lock.tag))
        .map_err(|e| EngineError::RuntimeInstall(e.to_string()))
}

/// Associe une entrée du verrou à une archive publiée. Une seule archive doit
/// correspondre au motif, et son empreinte doit être connue.
fn plan_from_release(entry: &LockedArchive, assets: &[Asset]) -> Result<Planned> {
    let pattern = Regex::new(&entry.pattern).map_err(|e| EngineError::RuntimeInstall(e.to_string()))?;
    let matches: Vec<&Asset> = assets.iter().filter(|a| pattern.is_match(&a.name)).collect();
    let [asset] = matches.as_slice() else {
        return Err(EngineError::RuntimeInstall(format!(
            "{} published archive(s) match {}",
            matches.len(),
            entry.pattern
        )));
    };
    let published = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(str::to_owned);
    let sha256 =
        entry.sha256.clone().or(published).ok_or_else(|| {
            EngineError::RuntimeInstall(format!("no SHA-256 checksum is published for {}", asset.name))
        })?;
    Ok(Planned { file: asset.name.clone(), url: asset.browser_download_url.clone(), sha256 })
}

/// Résultat d'une installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InstalledRuntime {
    pub tag: String,
    pub target: String,
    /// Chemin de `llama-server`.
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub server: PathBuf,
    pub files: Vec<String>,
}

/// Télécharge et installe llama.cpp dans `dest` après consentement.
pub async fn install_runtime(
    gated: &hf_net::GatedClient,
    lock: &RuntimeLock,
    target: &str,
    dest: &Path,
    lang: Lang,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<InstalledRuntime> {
    let entries = lock
        .targets
        .get(target)
        .ok_or_else(|| EngineError::RuntimeInstall(format!("llama.cpp publishes no archive for {target}")))?;
    let api = release_api(lock)?;
    let purpose = match lang {
        Lang::Fr => format!(
            "Télécharger le moteur llama.cpp {} ({target}) depuis GitHub, quelques dizaines de Mo. \
             Aucune donnée n'est envoyée ; l'archive est vérifiée par son empreinte SHA-256.",
            lock.tag
        ),
        Lang::En => format!(
            "Download the llama.cpp engine {} ({target}) from GitHub, a few tens of MB. \
             No data is sent; the archive is verified against its SHA-256 checksum.",
            lock.tag
        ),
    };
    let action = hf_net::NetAction::new(hf_net::ActionKind::RuntimeDownload, purpose, vec![api.clone()])
        .with_extra_hosts(DOWNLOAD_HOSTS);
    let session = gated.begin(action).await?;

    // L'API n'est interrogée que si le verrou ne dit pas déjà tout.
    let mut assets: Option<Vec<Asset>> = None;
    let mut planned = Vec::new();
    for entry in entries {
        planned.push(match (&entry.file, &entry.sha256) {
            (Some(file), Some(sha256)) => {
                Planned { file: file.clone(), url: direct_url(lock, file)?, sha256: sha256.clone() }
            }
            _ => {
                if assets.is_none() {
                    assets = Some(session.get_json::<Release>(&api).await?.assets);
                }
                plan_from_release(entry, assets.as_deref().unwrap_or_default())?
            }
        });
    }
    let total: Option<u64> = assets
        .as_ref()
        .map(|all| planned.iter().filter_map(|p| all.iter().find(|a| a.name == p.file)).map(|a| a.size).sum::<u64>());

    let work = dest.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    let result = async {
        let mut done = 0u64;
        let mut archives = Vec::new();
        for item in &planned {
            let path = work.join("archives").join(safe_file_name(&item.file)?);
            let offset = done;
            let report = session
                .download(&item.url, &path, Some(&item.sha256), &|bytes, file_total| {
                    progress(offset + bytes, total.filter(|t| *t > 0).or(file_total.map(|t| offset + t)));
                })
                .await?;
            done += report.bytes;
            archives.push(path);
        }
        let (tag, target, work, dest) = (lock.tag.clone(), target.to_owned(), work.clone(), dest.to_path_buf());
        let planned = planned.clone();
        tokio::task::spawn_blocking(move || assemble(&archives, &planned, &tag, &target, &work, &dest))
            .await
            .map_err(|e| EngineError::RuntimeInstall(e.to_string()))?
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&work).await;
    result
}

/// Décompresse les archives vérifiées, garde les fichiers utiles et remplace
/// le dossier géré `dest`. L'ancien n'est retiré qu'une fois le nouveau prêt.
fn assemble(
    archives: &[PathBuf],
    planned: &[Planned],
    tag: &str,
    target: &str,
    work: &Path,
    dest: &Path,
) -> Result<InstalledRuntime> {
    let install = work.join("install");
    let mut files = Vec::new();
    for (i, archive) in archives.iter().enumerate() {
        let into = work.join("extracted").join(i.to_string());
        extract(archive, &into)?;
        files.extend(install_files(&into, &install)?);
    }
    files.sort();
    files.dedup();
    let exe = server_file_name(target);
    if !install.join(&exe).is_file() {
        return Err(EngineError::RuntimeInstall(format!("{SERVER_BINARY} is missing from the archive")));
    }
    let archives: Vec<_> = planned.iter().map(|p| serde_json::json!({ "file": p.file, "sha256": p.sha256 })).collect();
    let version = serde_json::json!({ "tag": tag, "target": target, "archives": archives });
    fs::write(install.join("VERSION.json"), serde_json::to_vec_pretty(&version)?)?;
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&install, dest)?;
    Ok(InstalledRuntime { tag: tag.to_owned(), target: target.to_owned(), server: dest.join(exe), files })
}

fn server_file_name(target: &str) -> String {
    if target.starts_with("win") { format!("{SERVER_BINARY}.exe") } else { SERVER_BINARY.to_owned() }
}

/// Un nom d'archive ne doit désigner qu'un fichier du dossier de travail.
fn safe_file_name(name: &str) -> Result<&str> {
    let ok = !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..";
    ok.then_some(name).ok_or_else(|| EngineError::RuntimeInstall(format!("unexpected archive name: {name}")))
}

/// Décompresse une archive `.tar.gz` ou `.zip` dans `into`, sans jamais
/// écrire en dehors de ce dossier.
pub fn extract(archive: &Path, into: &Path) -> Result<()> {
    fs::create_dir_all(into)?;
    let name = archive.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_ascii_lowercase();
    let file = fs::File::open(archive)?;
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(file).map_err(|e| EngineError::RuntimeInstall(e.to_string()))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| EngineError::RuntimeInstall(e.to_string()))?;
            // `enclosed_name` écarte les chemins absolus et les `..`.
            let Some(relative) = entry.enclosed_name() else { continue };
            let out = into.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(&out)?;
            } else if entry.is_file() {
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                std::io::copy(&mut entry, &mut fs::File::create(&out)?)?;
            }
        }
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in tar.entries()? {
            // `unpack_in` refuse les entrées qui sortiraient du dossier.
            entry?.unpack_in(into)?;
        }
    } else {
        return Err(EngineError::RuntimeInstall(format!("unsupported archive format: {name}")));
    }
    Ok(())
}

/// Fichiers à installer : le serveur, les bibliothèques partagées et les licences.
pub fn is_shipped(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == SERVER_BINARY || lower == format!("{SERVER_BINARY}.exe") {
        return true;
    }
    let shared_lib = lower.ends_with(".dll")
        || lower.ends_with(".dylib")
        || lower.split_once(".so").is_some_and(|(_, rest)| {
            rest.is_empty() || rest.strip_prefix('.').is_some_and(|v| v.split('.').all(|p| p.parse::<u32>().is_ok()))
        });
    shared_lib || ["license", "licence", "copying", "notice"].iter().any(|p| lower.starts_with(p))
}

/// Copie les fichiers utiles d'une archive décompressée vers `dest` (à plat)
/// et rend leurs noms. Les liens symboliques sont suivis seulement s'ils
/// restent dans l'archive.
pub fn install_files(extracted: &Path, dest: &Path) -> Result<Vec<String>> {
    fs::create_dir_all(dest)?;
    let root = extracted.canonicalize()?;
    let mut copied = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue };
            if !is_shipped(&name) {
                continue;
            }
            let Ok(real) = path.canonicalize() else { continue };
            if !real.starts_with(&root) || !real.is_file() {
                continue;
            }
            let target = dest.join(&name);
            fs::copy(&real, &target)?;
            make_executable(&target, &name)?;
            copied.push(name);
        }
    }
    Ok(copied)
}

#[cfg(unix)]
fn make_executable(path: &Path, name: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if name == SERVER_BINARY {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path, _name: &str) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn builtin_lock_covers_desktop_targets() {
        let lock = RuntimeLock::builtin();
        assert!(!lock.tag.is_empty() && lock.repo.contains('/'));
        for (os, arch) in [
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("windows", "aarch64"),
        ] {
            let key = platform_key(os, arch).unwrap();
            let entries = lock.targets.get(&key).unwrap_or_else(|| panic!("entrée manquante : {key}"));
            for entry in entries {
                Regex::new(&entry.pattern).unwrap();
                assert_eq!(entry.file.is_some(), entry.sha256.is_some(), "{key} : nom et empreinte vont ensemble");
            }
        }
        assert_eq!(platform_key("macos", "aarch64").unwrap(), "macos-arm64/metal");
        assert!(platform_key("freebsd", "x86_64").is_none());
        for host in DOWNLOAD_HOSTS {
            assert!(hf_net::host_allowed(&DOWNLOAD_HOSTS.map(String::from), host));
        }
    }

    fn asset(name: &str, digest: Option<&str>) -> Asset {
        Asset {
            name: name.into(),
            browser_download_url: Url::parse(&format!("https://github.com/o/r/releases/download/v1/{name}")).unwrap(),
            size: 10,
            digest: digest.map(str::to_owned),
        }
    }

    #[test]
    fn an_archive_is_planned_only_with_a_checksum() {
        let assets = [
            asset("llama-v1-bin-ubuntu-x64.tar.gz", Some("sha256:abc")),
            asset("llama-v1-bin-ubuntu-vulkan-x64.tar.gz", None),
        ];
        let entry = |pattern: &str, sha: Option<&str>| LockedArchive {
            pattern: pattern.into(),
            file: None,
            sha256: sha.map(str::to_owned),
        };

        let planned = plan_from_release(&entry(r"-bin-ubuntu-x64\.(tar\.gz|zip)$", None), &assets).unwrap();
        assert_eq!((planned.file.as_str(), planned.sha256.as_str()), ("llama-v1-bin-ubuntu-x64.tar.gz", "abc"));
        // L'empreinte du verrou l'emporte sur celle publiée.
        let pinned = plan_from_release(&entry(r"-bin-ubuntu-x64\.(tar\.gz|zip)$", Some("def")), &assets).unwrap();
        assert_eq!(pinned.sha256, "def");
        // Pas d'empreinte : refus. Motif ambigu ou sans résultat : refus.
        assert!(plan_from_release(&entry(r"-bin-ubuntu-vulkan-x64", None), &assets).is_err());
        assert!(plan_from_release(&entry(r"-bin-ubuntu-", None), &assets).is_err());
        assert!(plan_from_release(&entry(r"-bin-win-", None), &assets).is_err());
    }

    #[test]
    fn shipped_files_are_the_server_its_libraries_and_licences() {
        for name in [
            "llama-server",
            "llama-server.exe",
            "libllama.so",
            "libggml.so.0.9",
            "ggml-vulkan.dll",
            "libmtmd.dylib",
            "LICENSE",
        ] {
            assert!(is_shipped(name), "{name}");
        }
        for name in ["llama-cli", "rpc-server.exe", "README.md", "something.sock", "libfoo.so.bak"] {
            assert!(!is_shipped(name), "{name}");
        }
        assert!(safe_file_name("a.tar.gz").is_ok());
        assert!(safe_file_name("../a").is_err() && safe_file_name("a/b").is_err() && safe_file_name("..").is_err());
    }

    fn tar_gz(path: &Path, files: &[(&str, &str)]) {
        let gz = flate2::write::GzEncoder::new(fs::File::create(path).unwrap(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for (name, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, content.as_bytes()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn tar_and_zip_archives_install_flat() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("llama-v1-bin-ubuntu-x64.tar.gz");
        tar_gz(
            &archive,
            &[
                ("llama-v1/llama-server", "#!/bin/sh"),
                ("llama-v1/llama-cli", "x"),
                ("llama-v1/lib/libllama.so", "lib"),
                ("llama-v1/LICENSE", "MIT"),
            ],
        );
        let (staged, dest) = (tmp.path().join("x"), tmp.path().join("dest"));
        extract(&archive, &staged).unwrap();
        let mut files = install_files(&staged, &dest).unwrap();
        files.sort();
        assert_eq!(files, ["LICENSE", "libllama.so", "llama-server"]);
        assert!(!dest.join("llama-cli").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(dest.join("llama-server")).unwrap().permissions().mode() & 0o111, 0o111);
        }

        let zip_path = tmp.path().join("llama-v1-bin-win-cpu-x64.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in [("llama-server.exe", "MZ"), ("llama.dll", "dll"), ("../evil.dll", "no")] {
            zip.start_file(name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let (staged, dest) = (tmp.path().join("z"), tmp.path().join("zdest"));
        extract(&zip_path, &staged).unwrap();
        let mut files = install_files(&staged, &dest).unwrap();
        files.sort();
        assert_eq!(files, ["llama-server.exe", "llama.dll"]);
        assert!(!tmp.path().join("evil.dll").exists(), "rien n'est écrit hors du dossier d'extraction");

        assert!(extract(&tmp.path().join("x.rar"), &tmp.path().join("r")).is_err());
    }

    #[test]
    fn assemble_replaces_the_managed_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let (work, dest) = (tmp.path().join("work"), tmp.path().join("runtimes").join("llama-cpp"));
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old-file"), "old").unwrap();
        let archive = work.join("a.tar.gz");
        tar_gz(&archive, &[("bin/llama-server", "new"), ("bin/libllama.so", "lib")]);
        let planned =
            [Planned { file: "a.tar.gz".into(), url: Url::parse("https://github.com/x").unwrap(), sha256: "s".into() }];

        let installed =
            assemble(std::slice::from_ref(&archive), &planned, "v1", "linux-x64/cpu", &work, &dest).unwrap();
        assert_eq!(installed.server, dest.join("llama-server"));
        assert_eq!(installed.files, ["libllama.so", "llama-server"]);
        assert!(!dest.join("old-file").exists());
        let version: serde_json::Value = serde_json::from_slice(&fs::read(dest.join("VERSION.json")).unwrap()).unwrap();
        assert_eq!(version["archives"][0]["sha256"], "s");

        // Archive sans serveur : erreur, et l'installation en place est conservée.
        let empty = work.join("b.tar.gz");
        tar_gz(&empty, &[("bin/llama-cli", "x")]);
        let work2 = tmp.path().join("work2");
        fs::create_dir_all(&work2).unwrap();
        assert!(assemble(&[empty], &planned, "v2", "linux-x64/cpu", &work2, &dest).is_err());
        assert!(dest.join("llama-server").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_leaving_the_archive_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("secret.so");
        fs::write(&outside, "secret").unwrap();
        let staged = tmp.path().join("x");
        fs::create_dir_all(&staged).unwrap();
        fs::write(staged.join("llama-server"), "bin").unwrap();
        fs::write(staged.join("libllama.0.dylib"), "lib").unwrap();
        std::os::unix::fs::symlink("libllama.0.dylib", staged.join("libllama.dylib")).unwrap();
        std::os::unix::fs::symlink(&outside, staged.join("libevil.so")).unwrap();

        let mut files = install_files(&staged, &tmp.path().join("dest")).unwrap();
        files.sort();
        assert_eq!(files, ["libllama.0.dylib", "libllama.dylib", "llama-server"]);
    }
}
