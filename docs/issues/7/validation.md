# Validation — issue #7

Two real repositories indexed into one throwaway store, then queried with no further source walk.

| Repo | Index |
|------|--------|
| `../bindb` as `bindb` | 91 files, 1610 symbols, 1.2s |
| `.` as `codeintel` | 40 files, 262 symbols, 0.5s |

Commands:

```
codeintel index ../bindb --store ./fed-store --name bindb
codeintel index . --store ./fed-store --name codeintel
codeintel federation --store ./fed-store
codeintel federation --store ./fed-store --impact bindb
```

## Federation

| Field | Value |
|-------|--------|
| `source_files_opened` | **0** |
| Repos | bindb, codeintel |
| Cross-repo `DEPENDS_ON` | `codeintel/codeintel` → `bindb/bindb` |
| Cycles spanning repos | none |
| Impact `bindb` dependents | `codeintel/codeintel` |

## Tests

`tests/federation.rs` indexes two Git repos with a mutual named dep, deletes both trees, then asserts cross-repo edges, a spanning cycle, and impact. `cargo clippy --all-targets -- -D warnings` clean.

## Not in this milestone

crates.io resolution, rust-analyzer/rustc, one store per repo.
