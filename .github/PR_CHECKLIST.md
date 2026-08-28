# Pull Request Checklist (codeintel)

- [ ] Target GitHub issue exists (Issue Doctrine body)
- [ ] PR body names `#N` and chooses **Closes** vs **Updates** (`Implements`)
- [ ] Feature branch only (never commit on `master`)
- [ ] `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test` (and `make b` when dpk-build is available)
- [ ] No secrets in the tree
- [ ] Do not merge unless the human explicitly asks in the same message
