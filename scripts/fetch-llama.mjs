#!/usr/bin/env node
// Prépare `llama-server` pour le paquet de HarnessForge (outil de build).
//
// Ce script s'exécute sur la machine de build, jamais dans l'application :
// il télécharge l'archive officielle de llama.cpp fixée dans
// `scripts/llama.lock.json`, vérifie son empreinte SHA-256, puis copie
// `llama-server` et ses bibliothèques dans `src-tauri/resources/llama/`,
// que Tauri embarque dans l'installeur.
//
// Premier téléchargement d'une version : les noms d'archives et empreintes
// ne sont pas encore connus. Avec `--update-lock`, le script interroge l'API
// GitHub, choisit l'archive selon le motif du fichier de verrouillage,
// vérifie l'empreinte publiée par GitHub quand elle existe, et écrit nom et
// empreinte dans le fichier, à versionner. Sans `--update-lock`, une entrée
// incomplète est une erreur : un build ne fait jamais confiance au réseau.
//
// Usage :
//   node scripts/fetch-llama.mjs [--target <triple Rust>] [--variant cpu|vulkan|metal]
//                                [--update-lock] [--from <archive locale>]...
//
// --from permet un build sans réseau à partir d'archives déjà téléchargées
// (leurs empreintes sont vérifiées comme les autres).

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmodSync, cpSync, createReadStream, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// Surchargeables pour les tests du script.
const LOCK = process.env.HF_LLAMA_LOCK ?? join(ROOT, "scripts", "llama.lock.json");
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

/** Copie les fichiers livrés d'un dossier extrait ; rend la liste copiée. */
export function install(extracted, dest) {
  const files = walk(extracted);
  const server = files.find((f) => /^llama-server(\.exe)?$/.test(basename(f)));
  if (!server) throw new Error("llama-server introuvable dans l'archive");
  mkdirSync(dest, { recursive: true });
  const serverDir = dirname(server);
  const copied = [];
  for (const file of files) {
    const name = basename(file);
    // Bibliothèques à côté du serveur ; licences où qu'elles soient.
    if (!isShipped(name) || (dirname(file) !== serverDir && !/^(licen|copying|notice)/i.test(name))) continue;
    cpSync(file, join(dest, name), { dereference: true });
    copied.push(name);
  }
  return copied.sort();
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
  const args = { from: [], updateLock: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--") continue; // séparateur ajouté par `pnpm run … --`
    if (a === "--target") args.target = argv[++i];
    else if (a === "--variant") args.variant = argv[++i];
    else if (a === "--from") args.from.push(resolve(argv[++i]));
    else if (a === "--update-lock") args.updateLock = true;
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
  const key = resolveTargetKey(args);
  const entries = lock.targets[key];
  if (!entries) throw new Error(`aucune entrée « ${key} » dans scripts/llama.lock.json`);

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
      } else if (args.updateLock) {
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
        console.log(`  sha256 ${actual}${published ? " (identique à l'empreinte publiée)" : " (GitHub n'en publie pas : confiance au premier usage)"}`);
      } else {
        throw new Error(`l'entrée « ${key} » n'a pas encore d'empreinte : relancez avec --update-lock, vérifiez, puis versionnez scripts/llama.lock.json`);
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
    if (args.updateLock) writeFileSync(LOCK, `${JSON.stringify(lock, null, 2)}\n`);
    console.log(`llama.cpp ${lock.tag} (${key}) installé dans src-tauri/resources/llama : ${[...copied].join(", ")}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((e) => {
    console.error(`fetch-llama : ${e.message}`);
    process.exit(1);
  });
}
