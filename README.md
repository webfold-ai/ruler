# ruler

> Text layout kernel — `Ruler` (O(1) line-break fold), `Box` (glyph/space), AFM font metrics. Zero deps.

## What this is

ruler is the **general-purpose text layout kernel** for Ptyktos. It is **not** terminal-specific 

The font-metric layer is the **PDF base-14 AFM table** — Helvetica, Times, Courier, Symbol,
ZapfDingbats. These are the 5 built-in PDF fonts (PDF spec § 9.6.2.2), measured via the public
AFM standard width table. No font file, no embedding, no `fontdue`.

## Build

```sh
cargo build --release
cargo test
```
