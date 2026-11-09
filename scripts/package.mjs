#!/usr/bin/env node
// Construit les installeurs de HarnessForge pour le système courant.
//
//   Linux   : .deb, .rpm, .AppImage
//   macOS   : .app, .dmg
//   Windows : installeur NSIS (.exe) et .msi
//
// Étapes : vérification des outils, contrôles (garde-fou réseau, format,
// clippy, tests Rust et interface), préparation de llama-server, puis
// `tauri build`. Les empreintes SHA-256 des paquets sont écrites dans
// SHA256SUMS à côté d'eux.
//
// Usage :
//   node scripts/package.mjs [--target <triple Rust>] [--variant cpu|vulkan|metal]
//                            [--bundles deb,rpm,...] [--skip-checks] [--skip-llama]
//
// Signature (facultative) : les variables lues par la CLI Tauri sont
// transmises telles quelles (APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_PASSWORD,
// APPLE_TEAM_ID pour macOS ; certificat Windows via tauri.conf.json).
// Voir le README, section « Construire ».

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { resolveTargetKey } from "./fetch-llama.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const WINDOWS = process.platform === "win32";
const DEFAULT_BUNDLES = { linux: "deb,rpm,appimage", darwin: "app,dmg", win32: "nsis,msi" };
const MIN_RUST = [1, 95];

function parseArgs(argv) {
  const args = { checks: true, llama: true };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--") continue; // séparateur ajouté par `pnpm run … --`
    if (a === "--target") args.target = argv[++i];
    else if (a === "--variant") args.variant = argv[++i];
    else if (a === "--bundles") args.bundles = argv[++i];
    else if (a === "--skip-checks") args.checks = false;
    else if (a === "--skip-llama") args.llama = false;
    else throw new Error(`option inconnue : ${a}`);
  }
  return args;
}

function run(command, args, options = {}) {
  console.log(`\n› ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { cwd: ROOT, stdio: "inherit", shell: WINDOWS, ...options });
  if (result.status !== 0) throw new Error(`échec de : ${command} ${args.join(" ")}`);
}

function output(command, args) {
  const result = spawnSync(command, args, { cwd: ROOT, encoding: "utf8", shell: WINDOWS });
  return result.status === 0 ? result.stdout.trim() : null;
}

function checkTools() {
  const missing = [];
  const [major] = process.versions.node.split(".").map(Number);
  if (major < 22) missing.push(`Node.js ≥ 22 (trouvé ${process.versions.node})`);
  if (!output("pnpm", ["--version"])) missing.push("pnpm (corepack enable)");
  const rustc = output("rustc", ["--version"]);
  const version = rustc?.match(/(\d+)\.(\d+)/)?.slice(1).map(Number);
  if (!version || version[0] < MIN_RUST[0] || (version[0] === MIN_RUST[0] && version[1] < MIN_RUST[1])) {
    missing.push(`Rust ≥ ${MIN_RUST.join(".")} (trouvé ${rustc ?? "rien"})`);
  }
  if (process.platform === "linux" && output("pkg-config", ["--exists", "webkit2gtk-4.1"]) === null) {
    missing.push("webkit2gtk 4.1 (paquet libwebkit2gtk-4.1-dev ou équivalent)");
  }
  if (missing.length) throw new Error(`outils manquants :\n  - ${missing.join("\n  - ")}`);
}

function llamaReady(key) {
  const dir = join(ROOT, "src-tauri", "resources", "llama");
  const server = join(dir, WINDOWS || key.startsWith("win") ? "llama-server.exe" : "llama-server");
  if (!existsSync(server)) return false;
  try {
    return JSON.parse(readFileSync(join(dir, "VERSION.json"), "utf8")).target === key;
  } catch {
    return false;
  }
}

function walk(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    const stat = statSync(path);
    // Un .app est un dossier : on le liste sans y descendre.
    return stat.isDirectory() && !name.endsWith(".app") ? walk(path) : [path];
  });
}

function writeChecksums(bundleDir) {
  const files = walk(bundleDir).filter((f) => /\.(deb|rpm|AppImage|dmg|exe|msi)$/.test(f));
  const lines = files.map((f) => `${createHash("sha256").update(readFileSync(f)).digest("hex")}  ${basename(f)}`);
  if (lines.length) writeFileSync(join(bundleDir, "SHA256SUMS"), `${lines.join("\n")}\n`);
  return files;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  checkTools();

  if (args.checks) {
    run("pnpm", ["install", "--frozen-lockfile"]);
    run("node", ["scripts/check-network-deps.mjs"]);
    run("cargo", ["fmt", "--all", "--check"]);
    run("cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
    run("cargo", ["test", "--workspace"]);
    run("pnpm", ["typecheck"]);
    run("pnpm", ["test"]);
    run("pnpm", ["test:scripts"]);
  }

  const key = resolveTargetKey({ target: args.target, variant: args.variant });
  if (args.llama && !llamaReady(key)) {
    const fetchArgs = ["scripts/fetch-llama.mjs"];
    if (args.target) fetchArgs.push("--target", args.target);
    if (args.variant) fetchArgs.push("--variant", args.variant);
    run("node", fetchArgs);
  }
  if (!llamaReady(key)) {
    console.warn(`\nattention : llama-server (${key}) absent ; le paquet ne pourra utiliser que Ollama.`);
  }

  const bundles = args.bundles ?? DEFAULT_BUNDLES[process.platform];
  const tauriArgs = ["tauri", "build", "--bundles", bundles];
  if (args.target) tauriArgs.push("--target", args.target);
  run("pnpm", tauriArgs);

  const bundleDir = join(ROOT, "target", ...(args.target ? [args.target] : []), "release", "bundle");
  const files = writeChecksums(bundleDir);
  console.log(`\nPaquets (${bundleDir}) :\n${files.map((f) => `  - ${f.slice(bundleDir.length + 1)}`).join("\n")}`);
}

main().catch((e) => {
  console.error(`\npackage : ${e.message}`);
  process.exit(1);
});
