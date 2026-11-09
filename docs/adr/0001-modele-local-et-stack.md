# ADR 0001 — Modèle local, runtime et stack

- **Statut** : accepté le 2026-11-09
- **Contexte** : HarnessForge génère des AGENTS.md et des skills avec un LLM exécuté
  en local, 100 % hors-ligne par défaut, sur Windows, Linux et macOS.

## Décision

| Décision           | Choix retenu                                                                 |
| ------------------ | ---------------------------------------------------------------------------- |
| Modèle par défaut  | Qwen3.6-35B-A3B, quantifié Q4_K_M (≈ 22 à 24 Go RAM + VRAM cumulées)          |
| Modèle léger       | Qwen3.5-9B, quantifié Q4_K_M (≈ 6 à 7 Go)                                     |
| Alternative        | Gemma 4 26B-A4B (≈ 17 à 18 Go), proposé dans le catalogue                     |
| Runtime            | `llama-server` (llama.cpp) embarqué en sidecar sur les 3 OS ; Ollama et MLX en options |
| Stack              | Tauri 2 + cœur Rust + interface React/TypeScript                              |

## Critères

Par ordre de poids : raisonnement sur du code, mémoire requise, licence permissive
(Apache 2.0 ou MIT), support multi-OS, qualité en français et en anglais, mode
réflexion désactivable.

Profils matériels visés :

| Profil   | Machine type                                                   | Mémoire pour le modèle |
| -------- | -------------------------------------------------------------- | ---------------------- |
| Standard | PC 32 Go RAM + GPU 8 à 16 Go, ou Mac Apple Silicon 32 Go et plus | 20 à 28 Go             |
| Modeste  | PC 16 Go RAM sans GPU dédié, ou Mac 16 Go                        | 8 à 10 Go              |

## Comparatif (état au 2026-11-09)

| Modèle | Licence | Paramètres (actifs) | Mémoire Q4 (≈ Go) | Contexte | Mode réflexion | Code (/5) | FR (/5) | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| [Qwen3.6-35B-A3B](https://huggingface.co/Qwen/Qwen3.6-35B-A3B) | Apache 2.0 | 35B MoE (3B) | 22–24 | 262K | Oui, `enable_thinking` | 5 | 4 | **Par défaut** |
| [Qwen3.5-9B](https://huggingface.co/Qwen/Qwen3.5-9B) | Apache 2.0 | 9B dense | 6–7 | 262K | Oui, `enable_thinking` | 4 | 4 | **Léger** |
| [Gemma 4 26B-A4B](https://huggingface.co/google/gemma-4-26B-A4B-it) | Apache 2.0 | 25,2B MoE (3,8B) | 17–18 | 256K | Oui, jeton `<\|think\|>` | 4 | 5 | Alternative |
| [Gemma 4 E4B](https://ai.google.dev/gemma/docs/core/model_card_4) | Apache 2.0 | ≈ 4B effectifs | 4–5 | 128K | Oui | 3 | 4 | Alternative légère |
| [Ministral 3 14B Reasoning](https://mistral.ai/news/mistral-3) | Apache 2.0 | 14B dense | 9–10 | n.c. | Toujours active | 4 | 5 | Alternative FR |
| [gpt-oss-20b](https://huggingface.co/openai/gpt-oss-20b) | Apache 2.0 | 21B MoE (3,6B) | 14–16 | n.c. | Effort low/medium/high | 4 | 3 | Écarté |
| [Phi-4-reasoning](https://huggingface.co/microsoft/Phi-4-reasoning) | MIT | 14B dense | 9–10 | 32K | Toujours active | 4 | 1 | Écarté |
| [DeepSeek-R1-0528-Qwen3-8B](https://huggingface.co/deepseek-ai/DeepSeek-R1-0528-Qwen3-8B) | MIT | 8B dense | 5–6 | n.c. | Toujours active | 3 | 3 | Écarté |
| [Mistral Small 4](https://mistral.ai/news/mistral-small-4) | Apache 2.0 | 119B MoE (6B) | ≈ 70 | 256K | Effort none/high | 5 | 5 | Écarté (trop lourd) |

Les mémoires sont des estimations (Q4_K_M, contexte 8K). Les notes /5 sont une
appréciation tirée des fiches officielles et des classements publics ; le banc
d'essai `bench/` doit les confirmer sur notre tâche réelle.

## Justification

- **Qwen3.6-35B-A3B** : le plus orienté code et agents à cette taille ; 3B paramètres
  actifs, donc rapide même avec une partie des experts en RAM.
- **Qwen3.5-9B** : tient sur 16 Go, même interrupteur `enable_thinking` que le modèle
  par défaut, donc un seul jeu de prompts.
- **llama.cpp embarqué** : un format (GGUF), une API compatible OpenAI, aucun service
  tiers à installer, contrôle total du réseau. Ollama (service séparé, registre en
  ligne) et MLX (macOS seulement, runtime Python) restent des options.
- **Tauri 2** : installeur d'environ 10 à 20 Mo, RAM laissée au modèle, sidecar natif,
  et une CSP qui interdit tout accès réseau à l'interface.

## Écartés

- **Phi-4-reasoning** : « not intended to support multilingual use », contexte 32K.
- **DeepSeek-R1-0528-Qwen3-8B** : réflexion non désactivable, dépassé par Qwen3.5-9B.
- **gpt-oss-20b** : langues non documentées, pas de mode sans réflexion.
- **Mistral Small 4** : environ 70 Go en Q4, modèle de serveur.
- **Electron** : 90 à 150 Mo d'installeur, RAM en concurrence avec le modèle,
  Node a accès au réseau partout.
- **Python + Qt** : bundles lourds, `llama-cpp-python` à recompiler par OS et GPU.

## Conséquences

- Le modèle par défaut est une entrée de `resources/catalog/models.json` : le changer
  ne demande aucune modification de code.
- L'app détecte la mémoire disponible au premier lancement et propose le modèle adapté.
