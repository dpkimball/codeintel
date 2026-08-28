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

POC. Single Git repository. Rust parser first. Parser trait is language-agnostic.

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
```

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

Multi-repo, cross-repo deps, non-Rust parsers, embeddings, stored doctrine beyond the Binder layer profile, autonomous review loops.
