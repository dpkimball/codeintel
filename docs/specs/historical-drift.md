---
title: Historical architectural drift
status: poc
github_issue: https://github.com/dpkimball/codeintel/issues/5
---

# Historical architectural drift (POC #3)

POC #1 keeps the graph in sync with Git. POC #2 answers doctrine and impact from the **live** graph. This spec is about **time**.

Git remains the source of truth for source. bindb versions every object and edge. After two SHAs have been indexed, drift is a pair of `object_as_of` / `edge_as_of` queries — not a checkout and not a repository walk.

## What runs

| Command | Input | Store ops | Source |
|---------|--------|-----------|--------|
| `codeintel index` | Git HEAD | writes + stamps `code://commit/{name}/{sha}` with bindb commit id, crate names, file paths | Git blobs (index only) |
| `codeintel drift --from A --to B` | two indexed Git SHAs | `get` catalog objects; `object_as_of` crates/files; `edge_as_of` `DEPENDS_ON` | none |

`source_files_opened` in the JSON report is `0`.

## Catalog

Each indexed Git SHA gets a `CommitRecord` payload:

- `bindb_commit` — bindb `CommitId` at which that SHA's graph is current
- `crate_names` / `file_paths` — ids to probe with as_of (deterministic `code://crate/…` and `code://file/…`)

The catalog is how drift knows which ids existed. The graph itself is reconstructed from versioned objects and edges.

## Diff

- Crates / files added or removed (as_of presence)
- `DEPENDS_ON` edges added or removed (as_of on the hashed edge id)
- Doctrine findings introduced vs resolved (same rules as POC #2, evaluated on each reconstructed crate graph)

## Non-goals

Parser fidelity, rust-analyzer, rustc, multi-repo. Indexing still reads Git; **drift** must not.
