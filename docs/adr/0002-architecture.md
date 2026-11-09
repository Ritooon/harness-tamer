# ADR 0002 — Architecture de HarnessForge

- **Statut** : accepté le 2026-11-09
- **Dépend de** : [ADR 0001](0001-modele-local-et-stack.md)

## Décision

| Choix                        | Retenu                                                                                   |
| ---------------------------- | ---------------------------------------------------------------------------------------- |
| Processus                    | App Tauri + `llama-server` lancé en processus compagnon (sidecar)                         |
| Découpage Rust               | `hf-net` (seul accès réseau) et `hf-engine` (domaine, stockage, analyse, génération…)     |
| Accès réseau                 | Seul `hf-net` contient un client HTTP ; vérifié en CI (`scripts/check-network-deps.mjs`) |
| Consentement                 | Un dialogue natif par action utilisateur, qui liste les hôtes contactés                   |
| Politique réseau par défaut  | « Demander » ; « Coupé » disponible                                                       |
| Stockage                     | Un dossier par projet, JSON + Markdown, index de recherche en mémoire                     |
| Historique                   | Copie complète de l'AGENTS.md à chaque version                                            |
| Génération                   | Un appel au modèle par section ; la régénération partielle suit le même chemin            |
| Suppression                  | Vers la corbeille du système, après confirmation                                          |

## Schéma

```text
┌─ Machine de l'utilisateur ─────────────────────────────────────────────────┐
│ ┌─ Application HarnessForge (un processus) ─┐                               │
│ │ Interface React (WebView, CSP stricte)     │                               │
│ │        │ IPC typé                          │                               │
│ │ Commandes Tauri (dialogues natifs, sidecar)│                               │
│ │        │                                   │                               │
│ │ hf-engine (sans accès réseau) ─────────────┼──► Dépôt de l'utilisateur (lu)│
│ │        │                      ───────────┼──► Disque local (données)     │
│ │ hf-net (seul client HTTP) ─────────────────┼──► llama-server (127.0.0.1)   │
│ └────────┼───────────────────────────────────┘                               │
└──────────┼──────────────────────────────────────────────────────────────────┘
           └── dialogue natif, puis journal ──► Internet (à la demande)
```

## Verrous de la garantie hors-ligne

1. **Dépendances** : seul `hf-net` peut dépendre d'une bibliothèque HTTP, TLS ou WebSocket.
2. **Lint** : Clippy interdit `TcpStream`, `UdpSocket`, `TcpListener` et `Command` hors de `hf-net`.
3. **Interface** : la CSP n'autorise que l'IPC et les ressources locales ; les images distantes
   d'un aperçu Markdown sont bloquées.
4. **Permissions Tauri** : pas de plugin HTTP ; le plugin shell ne lance que `llama-server`.
5. **Modèle** : `llama-server` sur 127.0.0.1, port aléatoire, clé éphémère, `--offline`, `--no-webui`.
6. **Consentement** : dialogue natif déclenché côté Rust ; « Refuser » par défaut ; redirections
   limitées aux hôtes affichés.
7. **Aucun service de fond** : ni mise à jour automatique, ni rapport de plantage, ni mesure d'usage.

Chaque action réseau, acceptée ou refusée, est inscrite dans `network.log.jsonl`.

## Stockage

Voir `crates/hf-engine/src/store.rs` pour la référence exacte. Arborescence :

```text
<données>/
├── settings.json
├── network.log.jsonl
├── models/            catalog.json + *.gguf
├── runtimes/          builds optionnels (CUDA)
├── skills/<slug>/     skill.json, SKILL.md, ressources
└── projects/<uuid>/
    ├── project.json
    ├── repo-profile.json
    ├── brand/         fichiers importés + brand.json
    ├── AGENTS.md      version courante, avec marqueurs de section
    ├── history/       index.jsonl + NNNN.md
    └── skills/<slug>/ skills rattachés, copiés puis adaptés
```

## Génération

1. Analyse déterministe du dépôt (`repo-profile.json`), sans exécuter le code.
2. Contexte compact (formulaire, profil, charte, skills) avec un budget par bloc.
3. Plan fixe de 10 sections, un prompt par section en français et en anglais.
4. Un appel par section, réflexion activée seulement pour conventions, commandes, qualité, interdits.
5. Contrôles : commandes citées comparées au dépôt, sections vides ou manquantes.
6. Assemblage avec marqueurs `<!-- hf:section id="…" -->`, retirés à l'export, puis nouvelle version.
