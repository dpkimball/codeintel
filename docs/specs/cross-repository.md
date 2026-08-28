---
title: Cross-repository analysis
status: poc
github_issue: https://github.com/dpkimball/codeintel/issues/7
---

# Cross-repository analysis (POC #4)

POCs #1–#3 reason inside one Git repository. This spec is about **more than one repo in the same bindb store**.

Git remains the source of truth for each tree. Object ids already include the repo name (`code://crate/{repo}/{crate}`). Federation writes `DEPENDS_ON` when a crate’s declared dependency **name** matches a crate indexed under a different repo prefix. Query and impact then use those edges only.

## What runs

| Command | Input | Store ops | Source |
|---------|--------|-----------|--------|
| `codeintel index` (each repo) | Git tree | crate objects include `dependencies[]`; then cross-repo `DEPENDS_ON` | Git blobs (index only) |
| `codeintel federation --store …` | none | `list` `code://repo/` / `code://crate/`; `edges_from` `DEPENDS_ON` | none |
| `codeintel federation --store … --impact C` | crate or `repo/crate` | reverse `DEPENDS_ON` BFS | none |

`source_files_opened` is `0`. Matching is **name-based**, not crates.io or rustc.

## Non-goals

Parser fidelity, rust-analyzer, rustc, publishing to a registry to “prove” the dep. Path vs version in Cargo.toml does not matter; only the dependency **key** is stored.
