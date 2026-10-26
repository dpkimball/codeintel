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

Requires a sibling `../bindb` checkout (path dependency). Clone both repositories next to each other. No private registry is required. `Cargo.lock` matches bindb 0.1.2 on `master`.

```text
workspace/
  bindb/
  codeintel/
```

```bash
cd codeintel
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

`make b` is the local DPK build entrypoint. The commands above are the public build.

**Public binaries:** tag `v*` → [`codeintel-releases`](https://github.com/dpkimball/codeintel-releases). See [`docs/specs/private-source-public-binary.md`](docs/specs/private-source-public-binary.md).

**Doctrine:** default is portable (acyclic / flat). Binder Desktop layering is **opt-in** (`--profile binder` or `CODEINTEL_DOCTRINE_PROFILE=binder`); ranks live in `profiles/binder.toml`. There is no baked seed/repo URL list — pass paths on `index`.

## Index a repository

```bash
cargo run --release -- index /path/to/repo --store ./store --name my-repo
cargo run --release -- validate /path/to/repo --store ./store --name my-repo
```

Example queries (after index):

```bash
cargo run --release -- query --store ./store --name my-repo --question summary
cargo run --release -- query --store ./store --name my-repo --question cycles
cargo run --release -- query --store ./store --name my-repo --question layering
cargo run --release -- doctrine --store ./store --name my-repo
cargo run --release -- doctrine --store ./store --name my-repo --profile binder
cargo run --release -- architecture --store ./store summary
cargo run --release -- architecture --store ./store plan --request "change src/lib.rs"
```

Binder Desktop / multi-repo federation examples (operator overlay — not required defaults):

```bash
cargo run --release -- index ../binder-desktop --store ./store --name binder-desktop
cargo run --release -- index ../bindb --store ./store --name bindb
cargo run --release -- federation --store ./store
```

Agent API: [docs/specs/agent-architecture-api.md](docs/specs/agent-architecture-api.md). Inspect-set fidelity: [docs/specs/plan-inspect-sets.md](docs/specs/plan-inspect-sets.md). Intent / nested index: [docs/specs/plan-intent-nested.md](docs/specs/plan-intent-nested.md). Workflow: [docs/specs/autonomous-workflow.md](docs/specs/autonomous-workflow.md). Workflows: [docs/examples/agent-workflows.md](docs/examples/agent-workflows.md). Reasoning specs: [architectural-reasoning](docs/specs/architectural-reasoning.md), [historical-drift](docs/specs/historical-drift.md), [cross-repository](docs/specs/cross-repository.md). Discovery uses bindb; open source only at inspection-set paths. Drift requires both SHAs indexed. Federation requires each repo in the **same** store.

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

## What a store contains

`index` writes structural records: paths, symbol names, kinds, and hashes. It does not store full source text.

`index-issues` and `index-prs` do store GitHub issue and pull-request titles, URLs, labels, state, and body text truncated to 2,000 characters. A store built from a private repository can therefore contain private issue and PR text.

The store is a directory you pass to `--store` (the WAL file is `commits.wal` inside it). Delete that directory to delete the store. Do not commit a populated store, and do not put a real `GITHUB_TOKEN` in examples or command arguments. The curl fallback reads `GITHUB_TOKEN` from the environment and passes it to curl on stdin, not in the process argument list.

## Stability

0.1.x. The `code://` object layout and CLI flags can change before 1.0. Build against a sibling bindb checkout, as documented above.

## Contributing

Issues and pull requests are welcome. The maintainer reviews pull requests and merges the ones that land. Security reports go through [GitHub Security Advisories](https://github.com/dpkimball/codeintel/security/advisories/new). See [SECURITY.md](SECURITY.md).

## License

MIT. See [LICENSE](LICENSE).

## Out of scope (POC)

Multi-repo in **separate** stores, rust-analyzer/rustc, embeddings. Parser accuracy is a later enhancement; it is not required for doctrine, impact, drift, federation, or the workflow loop.
