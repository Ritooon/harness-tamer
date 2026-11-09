# HarnessForge

Application de bureau (Windows, Linux, macOS) qui rédige les fichiers Markdown
destinés aux agents de code : un `AGENTS.md` complet par projet et des skills
(`SKILL.md`). Le texte est produit par un modèle de langage qui tourne sur votre
machine. **Aucune donnée ne la quitte.**

- Projets actifs et archivés, recherche, tri.
- Formulaire de projet : plateforme, systèmes cibles, technologies imposées, dépôt
  existant analysé localement, charte graphique importée, contraintes.
- `AGENTS.md` en 10 sections (contexte, objectifs, stack, structure, commandes,
  conventions, qualité, interdits, Git, interface) : éditeur Markdown avec aperçu,
  régénération d'une seule section, historique des versions avec comparaison et
  restauration, vérifications automatiques.
- Skills : création manuelle ou assistée, bibliothèque locale, rattachement aux
  projets, adaptation au projet par le modèle, recherche et import en ligne sous
  consentement.
- Export d'un `AGENTS.md` propre et d'un dossier `.agents/` (dans un dossier ou en ZIP).
- Interface en français et en anglais, thème clair et sombre.

## Garanties de confidentialité

Deux règles, appliquées par le code et vérifiées en intégration continue :

1. **100 % hors ligne par défaut.** Aucune donnée (prompt, projet, télémétrie) ne
   quitte la machine, jamais.
2. **Internet seulement à votre demande**, après une boîte de dialogue native à
   chaque requête, et chaque accès est journalisé.

Les seules actions qui peuvent toucher Internet sont celles que vous lancez
vous-même : télécharger un modèle, chercher ou importer un skill en ligne, ouvrir un
lien. Chacune ouvre un dialogue qui dit ce qui va être fait et quels hôtes seront
contactés. Rien ne part tant que vous n'avez pas cliqué sur « Autoriser ». Dans les
réglages, la politique « Coupé » bloque même ces actions. Il n'existe pas de mode
« toujours autoriser ».

### Les sept verrous

| # | Verrou | Où le vérifier |
| - | ------ | -------------- |
| 1 | Seul le crate `hf-net` peut dépendre d'une bibliothèque HTTP, TLS, WebSocket ou d'un transport Git. Le graphe est contrôlé pour les six cibles de bureau. | `pnpm check:network` (`scripts/check-network-deps.mjs`) |
| 2 | Clippy interdit sockets et `Command::new` partout ; seul `hf-net` lève l'interdiction des sockets, explicitement. `llama-server` est lancé par le plugin shell de Tauri, depuis Rust uniquement. | `clippy.toml` |
| 3 | L'interface n'a aucun accès réseau. La CSP n'autorise que l'IPC et les fichiers de l'application. L'aperçu Markdown ignore le HTML et n'affiche aucune image. La fenêtre refuse toute navigation, nouvelle fenêtre ou téléchargement. | `src-tauri/tauri.conf.json`, `src-tauri/src/lib.rs` |
| 4 | Permissions Tauri minimales (`core:default`). L'interface n'a pas accès au shell, au système de fichiers, aux dialogues ni à l'ouverture de liens : tout passe par des commandes Rust. | `src-tauri/capabilities/default.json` |
| 5 | `llama-server` écoute sur 127.0.0.1, sur un port aléatoire, avec une clé éphémère. Il est lancé avec `--offline` et `--no-webui`. | `crates/hf-engine/src/llm/runtime.rs` |
| 6 | Le consentement est demandé côté Rust par un dialogue natif. Seul un clic sur « Autoriser » vaut accord. Les redirections sont limitées aux hôtes affichés. | `crates/hf-net`, `src-tauri/src/consent.rs` |
| 7 | Aucun service de fond : pas de mise à jour automatique, pas de rapport de plantage, pas de mesure d'usage. | absence de plugin updater, `Cargo.lock` |

Le journal `network.log.jsonl` enregistre chaque décision (acceptée, refusée,
bloquée) et chaque requête. Il se lit dans **Réglages › Réseau**. En CI, les tests
du cœur sont aussi exécutés dans un espace de noms réseau vide, qui ne contient que
la boucle locale.

**À savoir sur le dialogue.** Les boîtes de dialogue natives ne permettent pas
toujours de choisir le bouton qui a le focus clavier : selon le système, « Autoriser »
peut être le bouton par défaut. Fermer la fenêtre, appuyer sur Échap ou toute erreur
d'affichage valent refus. Mais un appui sur Entrée peut valoir accord si le système
place le focus sur « Autoriser ». Lisez le dialogue avant de valider.

### Où sont vos données

| Système | Dossier |
| ------- | ------- |
| Linux   | `~/.local/share/io.github.ritooon.harnessforge` |
| macOS   | `~/Library/Application Support/io.github.ritooon.harnessforge` |
| Windows | `%APPDATA%\io.github.ritooon.harnessforge` |

La variable d'environnement `HARNESSFORGE_DATA_DIR` permet de choisir un autre
dossier. Tout y est en JSON et en Markdown. Les suppressions passent par la corbeille
du système.

## Installation

Les installeurs sont produits par la CI (voir « Construire ») :

- **Linux** : `.deb`, `.rpm` ou `.AppImage`. Il faut WebKitGTK 4.1, présent par
  défaut sur les distributions récentes.
- **macOS 12 et plus** : `.dmg`. Tant que les paquets ne sont pas signés, ouvrez
  l'application une première fois avec clic droit › Ouvrir.
- **Windows 10 et 11** : installeur `.exe` (NSIS) ou `.msi`. L'installeur contient
  WebView2 et ne télécharge rien pendant l'installation. Tant qu'il n'est pas signé,
  SmartScreen demande une confirmation.

Chaque paquet contient `llama-server` (llama.cpp) mais aucun modèle, qui pèse
plusieurs gigaoctets et se télécharge à part.

## Choisir un modèle

Au premier lancement, **Réglages › Modèle local** affiche la mémoire de la machine
et recommande un modèle. Le comparatif complet et les raisons du choix sont dans
[l'ADR 0001](docs/adr/0001-modele-local-et-stack.md).

| Modèle (GGUF Q4_K_M) | Rôle | Fichier | Mémoire conseillée | Licence |
| -------------------- | ---- | ------- | ------------------ | ------- |
| Qwen3.6 35B-A3B | **par défaut** | 22,1 Go | 32 Go (RAM + VRAM) | Apache 2.0 |
| Qwen3.5 9B | **léger** | 5,7 Go | 12 Go | Apache 2.0 |
| Gemma 4 26B-A4B | alternative | 16,9 Go | 24 Go | Apache 2.0 |
| Ministral 3 14B Reasoning | alternative | 8,2 Go | 16 Go | Apache 2.0 |
| Gemma 4 E4B | très léger | 5,0 Go | 8 Go | Apache 2.0 |
| Qwen3.5 4B | très léger | 2,7 Go | 6 Go | Apache 2.0 |

En pratique :

- **32 Go de mémoire ou plus** (ou un Mac Apple Silicon de 32 Go) : Qwen3.6 35B-A3B.
  C'est un modèle MoE avec 3 milliards de paramètres actifs, donc rapide même sur le
  processeur.
- **16 Go** : Qwen3.5 9B.
- **8 Go** : Gemma 4 E4B ou Qwen3.5 4B. La génération fonctionne, mais relisez
  davantage le résultat.

Trois façons d'obtenir un modèle :

1. **Télécharger** depuis le catalogue. Le dialogue de consentement annonce la taille
   et l'hôte (Hugging Face). Le fichier est vérifié par SHA-256 avant d'être utilisé,
   et un téléchargement interrompu reprend là où il s'était arrêté.
2. **Importer un fichier GGUF** déjà présent sur le disque, sans accès réseau. Le
   fichier n'est pas copié.
3. **Utiliser Ollama**, s'il est déjà installé et lancé sur `127.0.0.1:11434`.
   Choisissez « Ollama » dans **Moteur d'inférence** et indiquez le nom du modèle.
   Ollama est un service séparé, qui a sa propre politique réseau.

Réglages du moteur intégré :

- **Taille du contexte** : 32 768 tokens par défaut.
- **Couches sur le GPU** : laisser vide pour que llama.cpp décide.
- **Couches MoE gardées en RAM** (`--n-cpu-moe`) : utile quand la VRAM est trop
  petite pour tout le modèle.

Le `llama-server` livré est la build CPU sur Linux et Windows, et la build Metal sur
macOS Apple Silicon. Pour une accélération CUDA ou Vulkan, déposez votre propre build
de `llama-server` et ses bibliothèques dans `<données>/runtimes/llama/`. Elle est
alors utilisée en priorité.

## Utilisation

1. **Nouveau projet** : remplissez le formulaire. Si vous cochez « Code existant »,
   le dépôt est analysé localement : langages, manifestes, commandes, outils, CI,
   conventions, historique Git. Rien n'est exécuté.
2. **AGENTS.md › Générer** : le modèle rédige les dix sections une par une. Chaque
   génération crée une version. Les commandes citées sont comparées à celles du
   dépôt, et les sections vides ou coupées sont signalées.
3. **Éditer** et **Enregistrer la version**, ou **Régénérer la section** avec une
   consigne. **Historique** compare et restaure (une restauration crée une nouvelle
   version).
4. **Skills** : créez-les dans la bibliothèque, rattachez-les au projet, puis
   **Adapter au projet**. La recherche en ligne lit par défaut le dépôt public
   `anthropics/skills`. Les sources se configurent dans les réglages (dépôt GitHub ou
   registre JSON en HTTPS).
5. **Export** : `AGENTS.md` sans les marqueurs internes, plus `.agents/skills/…` et
   `.agents/harnessforge.json`, dans un dossier ou une archive ZIP. Un fichier existant
   n'est remplacé qu'après confirmation.

## Construire

### Prérequis

| | Linux | macOS | Windows |
| - | ----- | ----- | ------- |
| Rust | ≥ 1.95 (`rustup`) | ≥ 1.95 | ≥ 1.95 (MSVC) |
| Node.js | ≥ 22, avec `corepack enable` pour pnpm | idem | idem |
| Système | `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev build-essential file` | Xcode Command Line Tools | Visual Studio Build Tools (C++), WebView2 |

### Commandes

```bash
pnpm install
pnpm tauri dev                  # application en mode développement

# Contrôles
pnpm check:network              # verrou 1 : aucune bibliothèque réseau hors hf-net
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # cœur, réseau isolé, application
pnpm typecheck && pnpm test     # interface (Vitest)
pnpm test:scripts               # scripts de build

# Paquets pour le système courant (contrôles + llama-server + tauri build)
node scripts/package.mjs
node scripts/package.mjs --variant vulkan     # Linux/Windows : build Vulkan de llama.cpp
node scripts/package.mjs --target aarch64-apple-darwin --skip-checks
```

Les paquets et leur fichier `SHA256SUMS` sont écrits dans
`target/[<cible>/]release/bundle/`.

### llama-server

`scripts/fetch-llama.mjs` copie `llama-server` et ses bibliothèques dans
`src-tauri/resources/llama/`, que Tauri embarque. La version de llama.cpp, les
archives et leurs empreintes SHA-256 sont fixées dans `scripts/llama.lock.json`. Un
build échoue si une empreinte ne correspond pas.

Les empreintes ne sont pas encore renseignées. Pour une nouvelle version, une seule
fois :

```bash
node scripts/fetch-llama.mjs --update-lock          # par plateforme et variante
git add scripts/llama.lock.json
```

Le script vérifie l'empreinte publiée par GitHub quand elle existe, sinon il fait
confiance au premier téléchargement et le dit. Pour un build sans réseau, passez des
archives déjà téléchargées avec `--from <archive>` : leurs empreintes sont vérifiées
de la même façon.

### Signature

Facultative. `scripts/package.mjs` transmet à la CLI Tauri les variables
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` et `APPLE_TEAM_ID`. Pour
Windows, ajoutez `certificateThumbprint` ou `signCommand` dans
`bundle.windows` de `tauri.conf.json`.

### Intégration continue

`.github/workflows/ci.yml` lance tous les contrôles à chaque push, dont les tests du
cœur dans un espace de noms réseau vide. Sur une étiquette `v*` ou à la demande, il
construit les installeurs pour Linux x64, macOS arm64 et Windows x64.

Seul le build accède au réseau, l'application jamais : dépendances, archive
llama.cpp, et pour l'AppImage les outils `linuxdeploy`. Le WebView2 hors ligne est
aussi téléchargé au build.

## Architecture

```text
src/                 interface React + TypeScript (Vite, CodeMirror, react-markdown)
src-tauri/           application Tauri 2 : commandes, dialogues natifs, pilotage de llama-server
crates/hf-engine/    cœur sans réseau : stockage, analyse de dépôt, charte, génération, skills, export
crates/hf-net/       seul accès réseau : consentement, journal, client local pour le moteur
resources/           catalogue des modèles, prompts FR/EN
scripts/             garde-fou réseau, llama.cpp, paquets
docs/adr/            décisions d'architecture
```

Détails dans [l'ADR 0002](docs/adr/0002-architecture.md). Les types partagés entre
Rust et TypeScript sont générés par `pnpm bindings` dans `src/bindings/`.

## Points ouverts

- **Licence du projet** : pas encore choisie, aucun fichier `LICENSE`. À décider
  avant toute diffusion. Les modèles du catalogue sont sous Apache 2.0 et
  llama.cpp sous MIT.
- **Empreintes llama.cpp** : `scripts/llama.lock.json` fixe la version `v0.9.0`, mais
  les noms d'archives et les empreintes seront remplis au premier `--update-lock`. Les
  téléchargements GitHub n'étaient pas accessibles depuis l'environnement de
  développement.
- **Signature et notarisation** des paquets macOS et Windows : non configurées, donc
  non testées.
- **Banc d'essai des modèles** sur la tâche réelle (prévu par l'ADR 0001) : pas
  encore réalisé. Les notes du comparatif viennent des fiches publiques.
- **Focus clavier du dialogue de consentement** : voir plus haut.
