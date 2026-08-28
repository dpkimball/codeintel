# Validation — issue #1

Target: Binder Desktop at `/Volumes/SSD/dpk-workspace/binder-desktop`  
SHA: `e8ea032c18b58188bdc24ab7acf588527ca1e4d5`  
Command: `codeintel validate ../binder-desktop --store ./validation-store --name binder-desktop`  
Binary: `cargo build --release`

## Metrics

| Measure | Result |
|---------|--------|
| Initial index | **2811 ms** (301 files, 2348 symbols) |
| Incremental (same SHA, skip) | **30 ms** |
| Store size after first index | **12.8 MB** |
| Source bytes indexed | 6_497_275 |
| Semantic model bytes | 1_666_439 |
| Token estimate (bytes/4) | 1_624_318 → 416_609 (**~74% reduction**) |
| Summary query | 5 ms |
| Cycles | 0 ms (none found at crate graph) |
| Layering | 0 ms (3 Binder-doctrine violations) |
| Unused public APIs | 7 ms (473 name-based; includes re-exports) |
| Duplicates | 3 ms (95 structural groups) |
| Fan-in/out top | 124 ms |
| Hotspots | 3 ms |

Architecture summary: 28 crates, 165 Rust files, 153 modules, doctrine `binder`.

Layering violations reported (storage/domain depending upward):

- `binder-storage` → `binder-ingest`, `binder-sdk`
- `dpk-semantic` → `binder-events`

## Tests

`cargo test` and `cargo clippy --all-targets -- -D warnings` passed locally on `feat/code-intelligence-poc`.

## Not verified

- rustc-accurate call graphs (name-based resolution only)
- Non-Rust languages
- QA runner / Security Audit live registry until those PRs merge
