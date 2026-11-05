# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/) once it is released.

## [Unreleased]

### Added

- Contract tests (`tests/contracts.rs`) that compare `break_after`, `lines` and `glyph_at_x` with
  naive reference implementations on random box runs, plus unit tests for the base-14 widths, break
  classes, the Thai dictionary, bidi order and the fit ratio.
- `fuzz/`: two libFuzzer targets, `fuzz_layout` and `fuzz_text`, with invariant checks.
- `deny.toml`, the cargo-deny policy.
- `rust-version = "1.88"`, `repository`, `readme`, `keywords` and `categories` in `Cargo.toml`.
- License texts: `LICENSE-MIT` and `LICENSE-APACHE`.
- Governance and CI: README, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, CONTRIBUTORS, workflows, and
  OpenSSF and supply-chain documentation.

### Changed

- The `pdf-base14` feature now controls whether the `pdf_base14` module is compiled. Before, the
  module was always compiled and the feature had no effect. The default is unchanged.
- Documentation now says that only Helvetica, for printable ASCII, is implemented in `pdf_base14`.
  The earlier text described all five base-14 faces.
- The README opening, the `Cargo.toml` description and the crate docs now describe the crate as text
  geometry (prepared advances, greedy breaking, hit testing, paragraph-local updates). They no longer
  say it replaces a font library. There is no font parsing or shaping.

### Fixed

- The crate-level doctest imported the old crate name (`textfold`), so it failed. It now imports `ruler`.
- Intra-doc links to items that do not exist (`prefill`, a `fontdue` feature) were removed.

## [0.1.0]

Initial split of the layout kernel from a larger repository (`fold-split`). Not published.
