---
id: private-source-public-binary
title: codeintel — private source / public binary + portable doctrine
status: superseded
created: 2026-09-02
updated: 2026-10-05
owner: codeintel
github_issue: https://github.com/dpkimball/codeintel/issues/45
related:
  - https://github.com/dpkimball/codeintel/issues/45
  - https://github.com/dpkimball/binder-workspace/issues/194
  - https://github.com/dpkimball/bindb/issues/97
---

# codeintel — public artifact + portable doctrine

> **Superseded for source licensing.** The source repository is MIT (`LICENSE`). This document describes the earlier private-source / public-binary channel. See `docs/public-release-readiness.md`.

## Purpose

Match 017 Tier B: publish a **public binary** without exposing private source, and ship **no DPK seed inventory / silent product doctrine** in library defaults.

## Doctrine

| Mode | When | Behavior |
|------|------|----------|
| Portable (default) | `detect()`, `--profile auto` without env | `workspace` / flat — acyclic only |
| Binder overlay | `--profile binder` or `CODEINTEL_DOCTRINE_PROFILE=binder` | Layer ranks from `profiles/binder.toml` + Binder forbidden/ownership rules |

No auto-detect based on seeing `binder-ui` in the graph. No `dpk-*` ownership globs in the default path. Index targets are always CLI arguments — never a baked repo URL list.

## Public executable

```text
codeintel --version
codeintel --help
```

cargo-dist → [`dpkimball/codeintel-releases`](https://github.com/dpkimball/codeintel-releases). Release CI clones private `bindb` as `../bindb` (`GH_RELEASES_TOKEN` must read bindb + write releases).

Public install docs must match cargo-dist naming: `codeintel-<rust-target-triple>.tar.xz`, per-archive `*.tar.xz.sha256`, and `sha256.sum`.

### cargo-dist announcement URL drift

`github-releases-repo` uploads to the public repo, but cargo-dist still embeds **private** `dpkimball/codeintel` download URLs in the release body and `dist-manifest.json` hosting block (upstream: [axodotdev/cargo-dist#2252](https://github.com/axodotdev/cargo-dist/issues/2252)).

Mitigation: `.github/workflows/fix-codeintel-releases-announcement.yml` rewrites those URLs after a successful `Release` (requires `GH_RELEASES_TOKEN`).

## Out of scope

Open-sourcing private source; renaming `dpk.toml` as a config format (that is the deliver manifest name from the build toolchain).
