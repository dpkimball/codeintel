# CLAUDE.md

Guidance for agents working in `codeintel`.

## What this crate is

Code-intelligence **domain** that writes a Git-synced semantic graph into **bindb**. It is not the storage engine. Do not add WAL, query-IR, or index internals here — those live in `../bindb`.

Git is the source of truth. bindb is the derived semantic representation.

## Layout

```
src/
  git_sync.rs      # incremental git ls-tree / blob hashes
  parser.rs        # language-agnostic FileIr
  rust_parser.rs   # syn visitor (Rust plugin)
  cargo_graph.rs   # workspace crates + path deps
  indexer.rs       # extract → bindb objects/edges
  query.rs         # agent questions over bindb
  doctrine.rs      # conformance engine (layering, forbidden, ownership, acyclic)
  impact.rs        # change impact + inspection set from graph
  report.rs        # validation metrics
  main.rs          # index / query / doctrine / impact / validate CLI
```

New languages implement `LanguageParser` and get wired in `indexer.rs`. Do not special-case a second language inside `rust_parser.rs`.

## Object namespace

All writes use actor `codeintel` and scheme `code://`. Edge types are constants in `ids.rs`.

## Build

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
make b
```

Path-depends on `../bindb`. CI checks out `bindb` as a sibling via `path_dep_repo`.

## Logging / secrets

Do not log file contents. Store payloads are structural JSON (names, kinds, hashes), not source text.
