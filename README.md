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

POC #1: incremental Git sync. POC #2: doctrine and impact. Historical drift and cross-repository federation are on master. Agent API: `Architecture`. Autonomous workflow: `docs/specs/autonomous-workflow.md` (`workflow start` / `workflow finish`).

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
cargo run --release -- architecture --store ./store summary
cargo run --release -- architecture --store ./store plan --request "change binder-storage"
cargo run --release -- workflow start --store ./store --request "change src/lib.rs" --bind codeintel=. --session /tmp/wf.json
cargo run --release -- workflow finish --session /tmp/wf.json --opened src/lib.rs --modified src/lib.rs --validation pass
```

Agent API: [docs/specs/agent-architecture-api.md](docs/specs/agent-architecture-api.md). Inspect-set fidelity: [docs/specs/plan-inspect-sets.md](docs/specs/plan-inspect-sets.md). Workflow: [docs/specs/autonomous-workflow.md](docs/specs/autonomous-workflow.md). Workflows: [docs/examples/agent-workflows.md](docs/examples/agent-workflows.md). Reasoning specs: [architectural-reasoning](docs/specs/architectural-reasoning.md), [historical-drift](docs/specs/historical-drift.md), [cross-repository](docs/specs/cross-repository.md). Discovery uses bindb; open source only at inspection-set paths. Drift requires both SHAs indexed. Federation requires each repo in the **same** store.

## bindb mapping

Not SQL tables. Native bindb objects and edges under `code://`:

| Logical entity | ObjectId |
|----------------|----------|
| repository | `code://repo/{name}` |
| git commit | `code://commit/{name}/{sha}` |
| file | `code://file/{name}/{path}` |
| module | `code://mod/{name}/{path}` |
| crate | `code://crate/{name}/{crate}` |
| config key | `code://config/{name}/{path}/{key}` |
| symbol | `code://sym/{name}/{qualname}` |
| sync bookmark | `code://sync/{name}` |

Edges: `CONTAINS`, `DEFINED_IN`, `IMPORTS`, `CALLS`, `REFERENCES`, `IMPLEMENTS`, `DEPENDS_ON`, `AT_COMMIT`, `IN_REPO`, `PARENT_OF`.

Call resolution is name-based (not rustc/HIR). Good enough to answer architectural questions; not a type checker.

## Out of scope (POC)

Multi-repo in **separate** stores, rust-analyzer/rustc, embeddings. Parser accuracy is a later enhancement; it is not required for doctrine, impact, drift, federation, or the workflow loop.
