# Validation — issue #5

Two Binder Desktop Git SHAs were indexed into a throwaway store (worktree for the older SHA, then the live tree). The worktree was removed **before** drift. Drift did not open application source.

| SHA | Role |
|-----|------|
| `a8ea7f1` | from — dynamic plugin loader |
| `df9a800` | to — plugin lifecycle wiring |

Commands:

```
codeintel index <worktree-at-from> --store ./drift-store --name binder-desktop
codeintel index ../binder-desktop --store ./drift-store --name binder-desktop
# remove worktree
codeintel drift --store ./drift-store --name binder-desktop --from a8ea7f1 --to df9a800
```

## Drift

| Field | Value |
|-------|--------|
| `source_files_opened` | **0** |
| `primary_source` | `bindb` |
| Crates added/removed | none |
| Files added/removed | none (same paths; blobs changed) |
| `DEPENDS_ON` added | `binder-binders` → `binder-sdk` |
| Doctrine introduced/resolved | none |

Second index was incremental: 7 files / 292 unchanged (178 ms) after a 2.6s first index.

## Tests

`tests/drift.rs` indexes two commits, deletes the Git fixture, then diffs crates, deps, and introduced acyclic doctrine. `cargo clippy --all-targets -- -D warnings` clean.

## Not in this milestone

rust-analyzer / rustc. Cross-repository. Parser fidelity.
