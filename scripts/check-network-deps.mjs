#!/usr/bin/env node
// Garde-fou de la garantie hors-ligne (ADR 0002, verrou n°1).
//
// Vérifie qu'aucun crate du workspace, hormis `hf-net`, ne dépend — même
// indirectement — d'une bibliothèque capable d'ouvrir une connexion réseau
// (client HTTP, TLS, WebSocket, transport Git). Les chemins qui passent par
// `hf-net` sont autorisés : c'est le seul point d'accès réseau.
//
// Usage : node scripts/check-network-deps.mjs
// Code de sortie 1 si une dépendance interdite est trouvée.

import { execFileSync } from "node:child_process";

const FORBIDDEN = new Set([
  "reqwest", "hyper", "hyper-util", "hyper-rustls", "hyper-tls", "ureq", "curl", "curl-sys",
  "isahc", "surf", "attohttpc", "h2", "h3", "quinn", "tokio-tungstenite", "tungstenite",
  "async-tungstenite", "websocket", "native-tls", "tokio-native-tls", "rustls", "tokio-rustls",
  "openssl", "openssl-sys", "gix-transport", "gix-protocol",
]);
const GATEWAY = "hf-net";

const metadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1"], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 }),
);
const nameOf = new Map(metadata.packages.map((p) => [p.id, p.name]));
const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));

/** Dépendances normales et de build (les dépendances de test sont exclues). */
function runtimeDeps(node) {
  return node.deps
    .filter((d) => d.dep_kinds.some((k) => k.kind === null || k.kind === "build"))
    .map((d) => d.pkg);
}

const violations = [];
for (const memberId of metadata.workspace_members) {
  const member = nameOf.get(memberId);
  if (member === GATEWAY) continue;
  // Parcours en largeur, en mémorisant le chemin pour un message lisible.
  const queue = [[memberId, [member]]];
  const seen = new Set([memberId]);
  while (queue.length > 0) {
    const [id, path] = queue.shift();
    for (const depId of runtimeDeps(nodes.get(id))) {
      const name = nameOf.get(depId);
      if (name === GATEWAY || seen.has(depId)) continue;
      seen.add(depId);
      const depPath = [...path, name];
      if (FORBIDDEN.has(name)) {
        violations.push(depPath.join(" -> "));
        continue;
      }
      queue.push([depId, depPath]);
    }
  }
}

if (violations.length > 0) {
  console.error("Dépendances réseau interdites hors de hf-net :");
  for (const v of violations) console.error(`  ${v}`);
  process.exit(1);
}
console.log(`OK : seul ${GATEWAY} dépend d'une bibliothèque réseau (${metadata.workspace_members.length} crates vérifiés).`);
