#!/usr/bin/env node
// Prépare `llama-server` pour le paquet de HarnessForge (outil de build).
//
// Ce script s'exécute sur la machine de build, jamais dans l'application :
// il télécharge l'archive officielle de llama.cpp fixée dans
// `resources/catalog/llama.json`, vérifie son empreinte SHA-256, puis copie
// `llama-server` et ses bibliothèques dans `src-tauri/resources/llama/`,
// que Tauri embarque dans l'installeur. `tauri dev` et `tauri build` le
// lancent d'eux-mêmes (voir tauri.conf.json).
//
// Premier téléchargement d'une version : les noms d'archives et empreintes
// ne sont pas encore connus. Le script interroge alors l'API GitHub, choisit
// l'archive selon le motif du fichier de verrouillage, vérifie l'empreinte
// publiée par GitHub quand elle existe, et écrit nom et empreinte dans le
// fichier, à versionner. En intégration continue (variable CI), une entrée
// incomplète est une erreur sauf avec `--update-lock` : un build publié ne
// fait jamais confiance au réseau.
//
// Usage :
//   node scripts/fetch-llama.mjs [--target <triple Rust>] [--variant cpu|vulkan|metal]
//                                [--update-lock] [--if-missing] [--optional]
//                                [--from <archive locale>]...
//
// --if-missing  ne fait rien si la bonne version est déjà en place.
// --optional    un échec (pas de réseau…) n'arrête pas le build : l'application
//               proposera alors d'installer le moteur elle-même.
// --from        build sans réseau à partir d'archives déjà téléchargées
//               (leurs empreintes sont vérifiées comme les autres).

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmodSync, cpSync, createReadStream, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// Surchargeables pour les tests du script.
const LOCK = process.env.HF_LLAMA_LOCK ?? join(ROOT, "resources", "catalog", "llama.json");
const DEST = process.env.HF_LLAMA_DEST ?? join(ROOT, "src-tauri", "resources", "llama");

/** Triple Rust → plateforme llama.cpp. */
const TRIPLES = {
  "x86_64-unknown-linux-gnu": "linux-x64",
  "aarch64-unknown-linux-gnu": "linux-arm64",
  "x86_64-apple-darwin": "macos-x64",
  "aarch64-apple-darwin": "macos-arm64",
  "x86_64-pc-windows-msvc": "win-x64",
  "aarch64-pc-windows-msvc": "win-arm64",
};

/** Plateforme de la machine courante, au format des clés du verrou. */
export function hostPlatform(platform = process.platform, arch = process.arch) {
  const os = { linux: "linux", darwin: "macos", win32: "win" }[platform];
  const cpu = { x64: "x64", arm64: "arm64" }[arch];
  if (!os || !cpu) throw new Error(`plateforme non prise en charge : ${platform}/${arch}`);
  return `${os}-${cpu}`;
}

/** Variante par défaut : Metal sur Mac Apple Silicon, CPU ailleurs (fonctionne partout). */
export function defaultVariant(platformKey) {
  return platformKey === "macos-arm64" ? "metal" : "cpu";
}

export function resolveTargetKey({ target, variant } = {}) {
  const platformKey = target ? TRIPLES[target] : hostPlatform();
  if (!platformKey) throw new Error(`triple Rust inconnu : ${target}`);
  return `${platformKey}/${variant ?? defaultVariant(platformKey)}`;
}

/** Choisit l'archive publiée correspondant au motif ; une seule doit convenir. */
export function pickAsset(assets, pattern) {
  const re = new RegExp(pattern);
  const matches = assets.filter((a) => re.test(a.name));
  if (matches.length !== 1) {
    const names = assets.map((a) => `  - ${a.name}`).join("\n");
    throw new Error(`${matches.length} archive(s) pour le motif ${pattern} ; archives publiées :\n${names}`);
  }
  return matches[0];
}

export async function sha256File(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

/** Fichiers à livrer : le serveur, ses bibliothèques partagées et les licences. */
export function isShipped(name) {
  const lower = name.toLowerCase();
  if (lower === "llama-server" || lower === "llama-server.exe") return true;
  if (/\.(dll|dylib)$/.test(lower) || /\.so(\.\d+)*$/.test(lower)) return true;
  return /^(license|licence|copying|notice)/.test(lower);
}

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

export function extract(archive, into) {
  mkdirSync(into, { recursive: true });
  if (archive.endsWith(".zip") && process.platform === "linux") {
    execFileSync("unzip", ["-q", "-o", archive, "-d", into]);
  } else {
    // bsdtar (macOS, Windows 10+) lit aussi les ZIP.
    execFileSync("tar", ["-xf", archive, "-C", into]);
  }
}

/** Copie à plat les fichiers livrés d'un dossier extrait ; rend la liste copiée. */
export function install(extracted, dest) {
  const files = walk(extracted);
  if (!files.some((f) => /^llama-server(\.exe)?$/.test(basename(f)))) throw new Error("llama-server introuvable dans l'archive");
  mkdirSync(dest, { recursive: true });
  const copied = new Set();
  for (const file of files) {
    const name = basename(file);
    if (!isShipped(name)) continue;
    cpSync(file, join(dest, name), { dereference: true });
    copied.add(name);
  }
  return [...copied].sort();
}

/**
 * Vrai si `dest` contient déjà llama-server pour cette version et cette cible.
 * Sans variante demandée, toute variante de la bonne plateforme convient
 * (une build Vulkan préparée à la main n'est pas remplacée par la build CPU).
 */
export function alreadyInstalled(dest, tag, key, anyVariant = false) {
  try {
    const version = JSON.parse(readFileSync(join(dest, "VERSION.json"), "utf8"));
    const server = join(dest, key.startsWith("win") ? "llama-server.exe" : "llama-server");
    const sameTarget = version.target === key || (anyVariant && String(version.target).split("/")[0] === key.split("/")[0]);
    return version.tag === tag && sameTarget && existsSync(server);
  } catch {
    return false;
  }
}

async function download(url, path, headers = {}) {
  const res = await fetch(url, { headers: { "User-Agent": "harnessforge-build", ...headers }, redirect: "follow" });
  if (!res.ok) throw new Error(`HTTP ${res.status} pour ${url}`);
  writeFileSync(path, Buffer.from(await res.arrayBuffer()));
}

async function releaseAssets(repo, tag) {
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "harnessforge-build" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const res = await fetch(`https://api.github.com/repos/${repo}/releases/tags/${encodeURIComponent(tag)}`, { headers });
  if (!res.ok) throw new Error(`API GitHub : HTTP ${res.status} pour la version ${tag}`);
  return (await res.json()).assets;
}

function parseArgs(argv) {
  const args = { from: [], updateLock: false, ifMissing: false, optional: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--") continue; // séparateur ajouté par `pnpm run … --`
    if (a === "--target") args.target = argv[++i];
    else if (a === "--variant") args.variant = argv[++i];
    else if (a === "--from") args.from.push(resolve(argv[++i]));
    else if (a === "--update-lock") args.updateLock = true;
    else if (a === "--if-missing") args.ifMissing = true;
    else if (a === "--optional") args.optional = true;
    else if (a === "--help" || a === "-h") args.help = true;
    else throw new Error(`option inconnue : ${a}`);
  }
  return args;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log(readFileSync(new URL(import.meta.url), "utf8").split("\n").filter((l) => l.startsWith("//")).join("\n"));
    return;
  }
  const lock = JSON.parse(readFileSync(LOCK, "utf8"));
  // Dans un hook de `tauri build --target …`, la cible vient de Tauri.
  args.target ??= TRIPLES[process.env.TAURI_ENV_TARGET_TRIPLE] ? process.env.TAURI_ENV_TARGET_TRIPLE : undefined;
  const key = resolveTargetKey(args);
  const entries = lock.targets[key];
  if (!entries) throw new Error(`aucune entrée « ${key} » dans resources/catalog/llama.json`);
  if (args.ifMissing && alreadyInstalled(DEST, lock.tag, key, !args.variant)) return;
  // Hors CI, une entrée incomplète est remplie au premier téléchargement.
  const mayFill = args.updateLock || !process.env.CI;
  let filled = false;

  const work = mkdtempSync(join(tmpdir(), "hf-llama-"));
  try {
    let assets = null;
    const archives = [];
    for (const [index, entry] of entries.entries()) {
      let path;
      if (args.from[index]) {
        path = args.from[index];
        if (entry.file && basename(path) !== entry.file) console.warn(`attention : ${basename(path)} ≠ ${entry.file} attendu`);
      } else if (entry.file && entry.sha256) {
        path = join(work, entry.file);
        console.log(`téléchargement de ${entry.file}…`);
        await download(`https://github.com/${lock.repo}/releases/download/${lock.tag}/${entry.file}`, path);
      } else if (mayFill) {
        assets ??= await releaseAssets(lock.repo, lock.tag);
        const asset = pickAsset(assets, entry.pattern);
        path = join(work, asset.name);
        console.log(`premier téléchargement de ${asset.name}…`);
        await download(asset.browser_download_url, path);
        const published = typeof asset.digest === "string" && asset.digest.startsWith("sha256:") ? asset.digest.slice(7) : null;
        const actual = await sha256File(path);
        if (published && published !== actual) throw new Error(`empreinte différente de celle publiée par GitHub pour ${asset.name}`);
        entry.file = asset.name;
        entry.sha256 = actual;
        filled = true;
        console.log(`  sha256 ${actual}${published ? " (identique à l'empreinte publiée)" : " (GitHub n'en publie pas : confiance au premier usage)"}`);
      } else {
        throw new Error(`l'entrée « ${key} » n'a pas encore d'empreinte : lancez le script avec --update-lock sur un poste de développement, puis versionnez resources/catalog/llama.json`);
      }
      if (entry.sha256) {
        const actual = await sha256File(path);
        if (actual !== entry.sha256) throw new Error(`empreinte SHA-256 incorrecte pour ${basename(path)} :\n  attendue ${entry.sha256}\n  obtenue  ${actual}`);
      }
      archives.push(path);
    }

    // Le README du dossier est versionné ; tout le reste est remplacé.
    mkdirSync(DEST, { recursive: true });
    for (const name of readdirSync(DEST)) if (name !== "README.md") rmSync(join(DEST, name), { recursive: true, force: true });
    const copied = new Set();
    for (const [i, archive] of archives.entries()) {
      const into = join(work, `x${i}`);
      extract(archive, into);
      for (const name of install(into, DEST)) copied.add(name);
    }
    if (copied.has("llama-server")) chmodSync(join(DEST, "llama-server"), 0o755);
    writeFileSync(
      join(DEST, "VERSION.json"),
      `${JSON.stringify({ repo: lock.repo, tag: lock.tag, target: key, archives: entries.map((e) => ({ file: e.file, sha256: e.sha256 })) }, null, 2)}\n`,
    );
    if (filled) {
      writeFileSync(LOCK, `${JSON.stringify(lock, null, 2)}\n`);
      console.log("resources/catalog/llama.json complété : pensez à le versionner (git add resources/catalog/llama.json).");
    }
    console.log(`llama.cpp ${lock.tag} (${key}) installé dans src-tauri/resources/llama : ${[...copied].join(", ")}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((e) => {
    const optional = process.argv.includes("--optional");
    console.error(`fetch-llama : ${e.message}`);
    if (optional) console.error("fetch-llama : le build continue sans llama-server ; l'application proposera de l'installer.");
    process.exit(optional ? 0 : 1);
  });
}
