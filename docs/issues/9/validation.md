# Validation — issue #9

Store: throwaway `/tmp/codeintel-poc-agent-store`. Agent API only (`codeintel architecture`).

## Binder Desktop

| Field | Value |
|-------|--------|
| Index | 299 files, 2329 symbols, 3.0s |
| `summary` `source_files_opened` | **0** |
| Crates / symbols | 28 / 2329 |
| `doctrine` findings | 5 |
| `layering` | 3 violations |
| `plan("change binder-storage")` | matched `binder-desktop/binder-storage`; 5 inspection files; dependents include app, ui, runtime, home, commands, binders |

## Federation (same store + bindb)

After `index ../bindb --name bindb`:

- Cross-repo `DEPENDS_ON`: `binder-app`, `binder-dpk`, `binder-runtime`, `dpk-semantic` → `bindb/bindb`
- Shared library: `bindb`
- `source_files_opened`: **0**

## Tests

`tests/architecture.rs` deletes both Git trees then calls `summary`, `doctrine`, `impact`, `dependencies`, `cycles`, `federation`, `plan`. `make b` / clippy `-D warnings` clean.

## Not in this milestone

Cursor/IDE/hosted/UI. rustc. Parser fidelity.
