---
title: Architectural reasoning on the semantic layer
status: poc
github_issue: https://github.com/dpkimball/codeintel/issues/3
---

# Architectural reasoning (POC #2)

POC #1 proved the graph can stay in sync with Git. This spec is about **answering from bindb**.

Git remains the source of truth for source. bindb is the source of truth for **discovery**: which objects violate a doctrine, and which objects a change would touch. Source files are opened only after those object ids (and their `file_path` payloads) are known.

## What runs

| Command | Input | Store ops | Source |
|---------|--------|-----------|--------|
| `codeintel doctrine --store … --name …` | Doctrine profile | `list` / `get` / `edges_*` on `code://crate` `DEPENDS_ON` | none |
| `codeintel impact --store … --name … --target T` | crate, module, file path, or symbol qualname | same, plus symbol/file/module objects | none |

`source_files_opened` in both JSON reports is `0`. Inspection paths are strings copied from bindb payloads, not `std::fs` reads of the application tree.

## Doctrine

A `Doctrine` is a list of graph rules:

- **Layering** — ranked crates; a deeper rank must not `DEPENDS_ON` a shallower rank
- **Forbidden dependency** — named (or `prefix-*`) crate pair
- **Ownership** — owner crates may only depend on an allow-list
- **Acyclic** — no cycles in the crate `DEPENDS_ON` graph

The built-in `binder` profile encodes Binder Desktop layer ranks plus storage↛ingest/sdk contracts. `workspace` is acyclicity only. `--profile auto` picks binder when `binder-ui` / `binder-app` are present.

Each finding: rule id, human rule text, severity, evidence (edge type + endpoints), bindb object ids, remediation.

## Change impact

`impact --target` resolves `T` against bindb (crate name, file path, module path, then symbol qualname / suffix). It reports incoming/outgoing edges, `CALLS` callers, `REFERENCES`, `IMPLEMENTS`, reverse-reachable modules, reverse-reachable crate dependents, and a **minimal inspection set** (crates/modules/files/symbols implicated by those edges).

Transitive crate impact is BFS over reversed `DEPENDS_ON`, not a repository walk.

## Non-goals

Parser fidelity, rust-analyzer, rustc, multi-repo. Those do not block this milestone.
