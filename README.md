# ruler

Text layout kernel for Rust. A `Ruler` prepares a run of glyph boxes once, then breaks it into
lines at any width with an O(1) read per line, and hit-tests pixels back to caret positions.
Zero dependencies. `#![forbid(unsafe_code)]`.

> **Status: 0.1.0, unpublished (`publish = false`).** The API is small and tested against a
> naive reference, but it has had no external users and no release yet. Expect breaking changes.

## What it provides

| Item | What it does | Where |
|---|---|---|
| `Box` | One layout unit: `advance`, `stretch`, `can_break`, `must_break`. Constructors `glyph`, `space`, `hard_break`. | `src/lib.rs` |
| `Ruler::prepare(&[Box])` | Builds the prefix sums and the coordinate-to-break index. Done once per run. | `src/lib.rs` |
| `Ruler::break_after(start, width)` | End of the line that starts at box `start`. | `src/lib.rs` |
| `Ruler::line(start, end, width)` | Natural width and fit ratio of a line. | `src/lib.rs` |
| `Ruler::lines(width)` | All lines for a width, contiguous from box 0 to the end. | `src/lib.rs` |
| `Ruler::glyph_at_x(line_start, x)` | Hit test: caret index at pixel `x` from a line start. | `src/lib.rs` |
| `Document` | One `Ruler` per paragraph. `replace`, `insert` and `remove` re-fold only the paragraph that changed. | `src/lib.rs` |
| `break_class`, `WordDict`, `dict_breaks` | Break opportunities by code-point range. Thai and Lao use longest-match over a caller-supplied word list. | `src/lib.rs` |
| `char_dir`, `bidi_visual_order`, `bidi_reorder` | Reduced bidi: an LTR base, with each maximal RTL run reversed. | `src/lib.rs` |
| `pdf_base14` | Helvetica advance widths for printable ASCII, and `boxes(text, font_size)`. | `src/pdf_base14.rs` |

Example (this is also the crate-level doctest):

```rust
use ruler::{Box, Ruler};

let boxes = [
    Box::glyph(1.0), Box::glyph(1.0), Box::glyph(1.0),
    Box::space(1.0, 0.5),
    Box::glyph(1.0), Box::glyph(1.0), Box::glyph(1.0),
];
let ruler = Ruler::prepare(&boxes);   // accumulate once
let lines = ruler.lines(4.0);         // two lines, broken at the space
assert_eq!(lines.len(), 2);
let wide = ruler.lines(100.0);        // reflow at a new width: same ruler, no re-measure
assert_eq!(wide.len(), 1);
```

For text in Helvetica, build the boxes with `pdf_base14::boxes(text, font_size)`, where
`font_size` is in pixels.

## How the layout works

1. **Snap.** `prepare` rounds each advance to whole pixels and takes prefix sums `cum`. Advances are
   therefore integers; the crate does no sub-pixel layout. Scale advances yourself (for example by
   64) if you need finer resolution.
2. **Index.** `col_break[x]` holds the largest break index whose cumulative advance is at most `x`.
   `col_glyph[x]` holds the glyph whose cell contains pixel `x`. Both tables are filled in one pass.
3. **Break.** `break_after(start, w)` reads `col_break` at `floor(cum[start] + w)`. That gives the
   farthest break that fits, which is a greedy first-fit break. If no break fits, the line ends at the
   next break, so the line is overfull. A hard break always ends a line.
4. **Fit.** `line` reports `natural` (the sum of advances) and `fit = (width - natural) / stretch`,
   the Knuth–Plass adjustment ratio. The ratio is reported for the caller to use. The crate does not
   optimise the break choice with it.

Cost: `prepare` is O(n + total px) in time and memory, where n is the number of boxes. After that,
`break_after`, `line` and `glyph_at_x` are O(1) each, and `lines(w)` is O(number of lines). Reflowing
at a new width reuses the same `Ruler`.

## Limits

- **Greedy, not optimal.** Breaks are chosen first-fit. The crate does not minimise total badness or
  do Knuth–Plass paragraph optimisation.
- **Integer pixels.** Advances are rounded with `round()` in `prepare`.
- **Memory grows with total advance in pixels.** `prepare` allocates two `u32` tables of
  `total_px + 1` entries. The code does not cap this, so a caller-supplied advance or font size of
  around 10^9 px would need several GB. Callers must bound font size and input length. This is read
  from the code and has not been measured.
- **Simplified break classes.** `break_class` covers ASCII space, tab, hyphens and dashes, CJK
  ideographs, NBSP, and the mandatory separators. It is not the full UAX #14 table.
- **Simplified bidi.** Strong direction comes from Hebrew and Arabic code-point ranges. There is no
  weak or neutral resolution and no nested embedding levels.
- **Thai/Lao dictionary is caller data.** `dict_breaks` tries each length from the longest dictionary
  word down to 2 at every Thai or Lao character, so its cost grows with the longest word.
- **Helvetica only, printable ASCII.** `pdf_base14` implements Helvetica widths for 0x20..=0x7E. Times,
  Courier, Symbol and ZapfDingbats are not implemented. Any other character gets a default advance
  of 556 units, which is an approximation and not a metric.
- **No shaping.** No kerning, ligatures, combining marks or font parsing. The crate takes advances
  from the caller or from the AFM table.

## Build and test

```sh
cargo build --locked --all-targets
cargo test --locked                       # 13 contract tests + doctest
cargo clippy --locked --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

The minimum supported Rust version is **1.88** (`rust-version` in `Cargo.toml`). It was tested on 1.88.0
and 1.89.0. Older versions were not tested.

The contract tests in `tests/contracts.rs` check `break_after`, `lines` and `glyph_at_x` against
naive O(n²) reference implementations, on pseudo-random box runs. They also check the Helvetica
widths against the AFM table, the break classes, the Thai dictionary, the bidi order and the fit
ratio.

### Fuzzing

`fuzz/` contains two `libFuzzer` targets, both using `libfuzzer-sys`:

- `fuzz_layout`: bytes -> base-14 boxes -> `Ruler` -> lines and hit tests, with invariant checks.
- `fuzz_text`: bytes -> break classes, dictionary breaks and bidi order, with invariant checks.

```sh
cargo +nightly fuzz run fuzz_layout --fuzz-dir fuzz -- -max_total_time=120
cargo +nightly fuzz run fuzz_text   --fuzz-dir fuzz -- -max_total_time=120
```

These need nightly and `cargo-fuzz`. CI runs them (see `.github/workflows/fuzz.yml`).

## Security

Text that comes from outside is untrusted input. See [SECURITY.md](SECURITY.md) for how to report a
vulnerability and for the supported versions. The dependency policy is in `deny.toml`, and the
supply-chain setup is described in [docs/SUPPLY-CHAIN.md](docs/SUPPLY-CHAIN.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Contributors are
listed in [CONTRIBUTORS.md](CONTRIBUTORS.md). Release notes are in [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
