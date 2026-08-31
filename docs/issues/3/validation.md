---
title: Validation — issue #3
description: Validation evidence for codeintel issue #3.
category: reviews
---

# Validation — issue #3

Store: existing Binder Desktop index (`./validation-store`, no re-index, no Git walk).

Commands:

```text
codeintel doctrine --store ./validation-store --name binder-desktop
codeintel impact --store ./validation-store --name binder-desktop --target binder-storage
```

## Doctrine

| Field | Value |
|-------|--------|
| Profile | `binder` (auto-detected) |
| Rules evaluated | 5 |
| `source_files_opened` | **0** |
| Findings | 5 |

- 3× `binder.layering` (storage→sdk, storage→ingest, dpk-semantic→binder-events)
- 1× `binder.storage-not-ingest`
- 1× `binder.storage-not-sdk`

Each finding includes rule id, `DEPENDS_ON` evidence, `code://crate/…` object ids, severity, remediation.

## Change impact (`binder-storage`)

| Field | Value |
|-------|--------|
| `source_files_opened` | **0** |
| Incoming edges | 5 |
| Outgoing edges | 3 |
| Transitive crate dependents | binder-app, binders, commands, home, runtime, ui |
| Inspection files | 7 (payload paths from bindb, not `std::fs`) |

## Tests

`cargo test` (11 integration tests including 5 in `tests/reasoning.rs` that drop the Git fixture before evaluate/impact). `cargo clippy --all-targets -- -D warnings` clean.

## Not in this milestone

rust-analyzer / rustc. Cross-repository. Historical drift.
