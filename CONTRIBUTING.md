# Contributing to ruler

Thanks for your interest. This is a small, zero-dependency crate. Changes are reviewed for correctness
first, then for scope.

## Before you start

- Read [README.md](README.md), especially **Limits**. Many things the crate does not do are deliberate,
  and a change that adds one of them needs a design discussion first.
- Open an issue for anything that changes the public API or the layout results. Small fixes and doc
  corrections can go straight to a pull request.
- Report security problems privately, as described in [SECURITY.md](SECURITY.md). Do not open a public
  issue for them.

## Checks

Run these before opening a pull request. CI runs the same commands.

```sh
cargo fmt --all -- --check
cargo build --locked --all-targets
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo check --locked --manifest-path fuzz/Cargo.toml --all-targets
cargo deny check advisories bans licenses sources
```

Dependency policy is in `deny.toml`. The library has no dependencies, so adding one is a significant
change. Explain why in the pull request.

## Rules for code

- No `unsafe` code. The crate root has `#![forbid(unsafe_code)]`, and the fuzz crate must not add any.
- Layout queries that are documented as O(1) must stay O(1). A change that adds per-query work needs
  a note in the doc comment and in the README.
- If you change a layout result, extend the naive reference in `tests/contracts.rs` rather than only
  adding a hand-picked example.
- Keep the MSRV (`rust-version` in `Cargo.toml`) building. CI checks it.
- Keep fuzz targets to real input boundaries. Do not add a target that only exercises trivial code.

## Pull requests

- Keep each pull request to one logical change.
- Fill in the pull request template.
- A pull request needs an approving review from a code owner before it merges.

## License of contributions

Unless you say otherwise, any contribution you intentionally submit for inclusion in this project is
licensed under the MIT or Apache-2.0 license, as in [README.md](README.md), with no additional terms.
