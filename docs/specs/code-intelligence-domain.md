---
title: Code Intelligence Domain
status: poc
github_issue: https://github.com/dpkimball/codeintel/issues/1
---

# Code Intelligence Domain

Git remains the source of truth. **codeintel** is a bindb consumer that maintains a continuously synchronized semantic graph of one Git repository so agents can answer architectural questions without re-reading the tree.

## Scope (POC)

- One Git repository
- Rust parser (`syn`); `LanguageParser` is language-agnostic
- Incremental sync on commit SHA + blob hash
- Agent queries against bindb, not source

## Out of scope

Multi-repo, rustc/HIR type checking, embeddings, historical CLI snapshots (bindb versions already retain prior object versions), autonomous review loops.

## bindb model (not SQL)

| Logical table | bindb object |
|---------------|--------------|
| repositories | `code://repo/{name}` |
| commits | `code://commit/{name}/{sha}` |
| files | `code://file/{name}/{path}` |
| symbols | `code://sym/{name}/{qualname}` |
| modules | `code://mod/{name}/{path}` |
| crates | `code://crate/{name}/{crate}` |

Relationships are first-class edges: `CALLS`, `IMPLEMENTS`, `DEPENDS_ON`, `CONTAINS`, `DEFINED_IN`, `IMPORTS`, `AT_COMMIT`, `IN_REPO`, `PARENT_OF`.

## Sync flow

1. Detect HEAD SHA
2. Diff `git ls-tree` blob hashes vs last bookmark
3. Parse changed files only
4. Tombstone previous file symbols/edges
5. Insert updated objects
6. Resolve calls/impls by name
7. Mark `code://sync/{name}` synchronized

If SHA is unchanged, skip.

## Agent queries

Circular crate deps, unused public APIs (no CALLS/REFERENCES fan-in), Binder layer violations, structural duplicates, fan-in/out, callers, trait impls, module paths, hotspots, mermaid crate graph, architecture summary.

Call resolution is **name-based**, not typed.

## Validation target

Binder Desktop (`../binder-desktop`). Measure initial index time, incremental no-op time, store size, query latency, token estimate (source bytes/4 vs model bytes/4).
