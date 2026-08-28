# codeintel

<p align="center">
  <a href="https://github.com/dpkimball/codeintel/actions/workflows/ci.yml"><img src="https://github.com/dpkimball/codeintel/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="https://github.com/dpkimball/codeintel/actions/workflows/security-audit.yml"><img src="https://github.com/dpkimball/codeintel/actions/workflows/security-audit.yml/badge.svg" alt="Security Audit" /></a>
</p>

**codeintel** is the code-intelligence domain for [bindb](https://github.com/dpkimball/bindb).

Git remains the source of truth. codeintel incrementally indexes a repository into bindb as a semantic graph (files, modules, symbols, calls, crate dependencies). Agents query bindb instead of re-reading the tree.

```text
Git repository → Git sync → language parser → semantic extract → bindb → agent query
```

## Status

POC #1: incremental Git sync into bindb. POC #2: doctrine conformance and change impact from the graph (no repo walk). POC #3: historical drift between two indexed Git SHAs via bindb `as_of`. POC #4: cross-repository `DEPENDS_ON` in one store.

## Build

Requires a sibling `../bindb` checkout (same layout as binder-desktop).

```bash
cargo test
cargo clippy --all-targets -- -D warnings
make b    # lint + test + release build; no image / no deploy
```

## Index Binder Desktop

```bash
cargo run --release -- validate ../binder-desktop --store ./store --name binder-desktop
```

Queries (after index):

```bash
cargo run --release -- query --store ./store --name binder-desktop --question summary
cargo run --release -- query --store ./store --name binder-desktop --question cycles
cargo run --release -- query --store ./store --name binder-desktop --question layering
cargo run --release -- query --store ./store --name binder-desktop --question unused
cargo run --release -- query --store ./store --name binder-desktop --question diagram
cargo run --release -- doctrine --store ./store --name binder-desktop
cargo run --release -- impact --store ./store --name binder-desktop --target binder-storage
cargo run --release -- drift --store ./store --name binder-desktop --from <sha-a> --to <sha-b>
cargo run --release -- index ../bindb --store ./store --name bindb
cargo run --release -- index . --store ./store --name codeintel
cargo run --release -- federation --store ./store
cargo run --release -- federation --store ./store --impact bindb
```

Reasoning is documented in [docs/specs/architectural-reasoning.md](docs/specs/architectural-reasoning.md), [docs/specs/historical-drift.md](docs/specs/historical-drift.md), and [docs/specs/cross-repository.md](docs/specs/cross-repository.md). Discovery uses bindb; open source only at the inspection-set paths the graph returns. Drift requires both SHAs to have been indexed; it does not check them out. Federation requires each repo to have been indexed into the **same** store.

## bindb mapping

Not SQL tables. Native bindb objects and edges under `code://`:

| Logical entity | ObjectId |
|----------------|----------|
| repository | `code://repo/{name}` |
| git commit | `code://commit/{name}/{sha}` |
| file | `code://file/{name}/{path}` |
| module | `code://mod/{name}/{path}` |
| crate | `code://crate/{name}/{crate}` |
| symbol | `code://sym/{name}/{qualname}` |
| sync bookmark | `code://sync/{name}` |

Edges: `CONTAINS`, `DEFINED_IN`, `IMPORTS`, `CALLS`, `REFERENCES`, `IMPLEMENTS`, `DEPENDS_ON`, `AT_COMMIT`, `IN_REPO`, `PARENT_OF`.

Call resolution is name-based (not rustc/HIR). Good enough to answer architectural questions; not a type checker.

## Out of scope (POC)

Multi-repo in **separate** stores, rust-analyzer/rustc, embeddings, autonomous review loops. Parser accuracy is a later enhancement; it is not required for doctrine, impact, drift, or federation.
