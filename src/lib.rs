//! Text layout kernel — Ruler, Box, pdf-base14 AFM font metrics. Zero deps.
//!
//! The general-purpose text layout kernel for ptyktos. Not terminal-specific.
//! Used by `textfold` (terminal), `webfold/htmlfold` (HTML → text),
//! `webfold/pdfcn` (PDF → text), `biota/content/...` (document renderers).
//!
//! fontdue is fully subsumed (AFM table covers the 5 PDF base-14 fonts).

#![forbid(unsafe_code)]

//! # textfold — text layout as the B/U accumulator fold over glyph advances
//!
//! The collective invariant of **fontdue** (glyph metrics + rasterization) and **pretext** (segment
//! + layout), lifted with `sang adapt`: both source trees cluster as one kind (~1.0 bits/symbol), and
//! the shared spine is the **accumulator** — fontdue's core is parse+rasterize, pretext's core is
//! segment + `prepareWithSegment` + layout. That accumulator is the same commutative-monoid `fuse` /
//! `mean` proved for the Kalman filter in Sanguine's `HashTrinity`: layout and estimation are the
//! one B/U accumulator at different applicatives.
//!
//! This crate is that accumulator, **browser-free** (pretext outsources width measurement to the
//! browser's `canvas.measureText`; here advances come from the font directly):
//!
//! - [`Ruler::prepare`] accumulates the cumulative-advance prefix sum **once** — the seed (`fuse`) —
//!   and a coordinate→break index.
//! - [`Ruler::break_after`] finds a line break by a **direct coordinate read** into that index — O(1),
//!   no search (`Web.getNode`); the line's fit is the Knuth–Plass ratio `r = (width−natural)/stretch`
//!   = `x = B/U`. [`Ruler::glyph_at_x`] is the inverse read (pixel → glyph: hit-testing).
//! - Reflow at a new width re-reads the **same** ruler; it never re-measures (the streaming law); a
//!   viewport of K visible lines is O(K) — flat in document size.
//!
//! Feed it advances from anywhere. The `fontdue` feature provides [`prefill`] — segment + break-rules
//! + font advances (pretext's `prepare()` stage, browser-free) — so it works on real text, no browser.
//!
//! ```
//! use textfold::{Box, Ruler};
//! // three unit-wide glyphs, then a breakable space, then three more
//! let boxes = [Box::glyph(1.0), Box::glyph(1.0), Box::glyph(1.0),
//!              Box::space(1.0, 0.5),
//!              Box::glyph(1.0), Box::glyph(1.0), Box::glyph(1.0)];
//! let ruler = Ruler::prepare(&boxes);      // accumulate once
//! let lines = ruler.lines(4.0);            // break at width 4
//! assert_eq!(lines.len(), 2);              // breaks at the space
//! let wide = ruler.lines(100.0);           // reflow — same ruler, no re-measure
//! assert_eq!(wide.len(), 1);
//! ```


pub mod crossterm;
pub mod image;
pub mod terminal;
pub mod tui;

#[cfg(any(feature = "ratatui", feature = "textfold-ratatui"))]
pub mod ratatui_compat;

/// Re-exports the core layout types.
pub mod prelude {
    pub use super::tui::prelude::*;
    pub use super::{Box, Ruler};
}

/// One layout box — a glyph or cluster: the advance it consumes, how much it may stretch for
/// justification, and whether a line may break **after** it (a break opportunity, e.g. after a space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box {
    /// Horizontal advance (the width the box consumes on the line). Must be ≥ 0.
    pub advance: f32,
    /// How much this box can stretch when a line is justified (0 for a fixed glyph).
    pub stretch: f32,
    /// Whether a line break is *allowed* immediately after this box (a soft opportunity, UAX #14).
    pub can_break: bool,
    /// Whether a line break is *forced* after this box (a mandatory break — `\n`, paragraph end).
    pub must_break: bool,
}

impl Box {
    /// A fixed glyph: consumes `advance`, no stretch, no break after.
    #[must_use]
    pub fn glyph(advance: f32) -> Self {
        Box {
            advance,
            stretch: 0.0,
            can_break: false,
            must_break: false,
        }
    }
    /// A breakable space: consumes `advance`, can stretch by `stretch`, a line may break after it.
    #[must_use]
    pub fn space(advance: f32, stretch: f32) -> Self {
        Box {
            advance,
            stretch,
            can_break: true,
            must_break: false,
        }
    }
    /// A mandatory break (`\n`): zero-width, forces the line to end here.
    #[must_use]
    pub fn hard_break() -> Self {
        Box {
            advance: 0.0,
            stretch: 0.0,
            can_break: true,
            must_break: true,
        }
    }
}

/// How a character behaves at a line boundary (UAX #14, simplified to char ranges — no Unicode
/// tables, no JS). The segmentation half of "prefill": classify each codepoint's break role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Break {
    /// No break here (inside a word, NBSP, a combining mark).
    None,
    /// A break is *allowed* after this char (space, hyphen, CJK).
    Soft,
    /// A break is *forced* after this char (`\n`, line/paragraph separators).
    Mandatory,
}

/// Classify a character's line-break behavior — UAX #14 reduced to the common classes by codepoint
/// range. Zero-dependency, zero tables: covers Latin (space/tab/hyphen), CJK (break between ideographs),
/// NBSP (no break), and the mandatory separators. Full UAX #14 (all classes, dictionary scripts) is the
/// tabled extension — this is the browser-free core pretext gets from the platform.
#[must_use]
pub fn break_class(c: char) -> Break {
    match c {
        '\n' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}' => {
            Break::Mandatory
        }
        '\u{00A0}' | '\u{202F}' | '\u{2060}' => Break::None, // NBSP / narrow-NBSP / word-joiner
        ' ' | '\t' | '\u{200B}' => Break::Soft,              // space, tab, zero-width space
        '-' | '\u{2010}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '/' => Break::Soft, // hyphen/dash/slash
        c if is_ideograph(c) => Break::Soft, // CJK: break between ideographs
        _ => Break::None,
    }
}

/// CJK ideographs / kana / Hangul — characters that may break on either side.
#[must_use]
pub fn is_ideograph(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF   | // Hangul Jamo
        0x2E80..=0x2FFF   | // CJK radicals / Kangxi
        0x3040..=0x30FF   | // Hiragana + Katakana
        0x3400..=0x4DBF   | // CJK Ext A
        0x4E00..=0x9FFF   | // CJK Unified
        0xAC00..=0xD7AF   | // Hangul Syllables
        0xF900..=0xFAFF   | // CJK Compatibility
        0x20000..=0x2FA1F   // CJK Ext B+ / supplement
    )
}

/// Scripts written without spaces, where word boundaries need a dictionary (UAX #14 can't break them).
#[must_use]
pub fn is_dict_script(c: char) -> bool {
    matches!(c as u32, 0x0E00..=0x0E7F | 0x0E80..=0x0EFF) // Thai, Lao
}

/// A word dictionary for space-less scripts (Thai/Lao). Word boundaries are found by **longest-match**
/// — the framework membership read (a set lookup, the seed): at each position take the longest word in
/// the set. The wordlist is swappable data; full coverage is the full dictionary (or ICU's). This is
/// the layer UAX #14 leaves open.
pub struct WordDict {
    words: std::collections::HashSet<String>,
    max_chars: usize,
}

impl WordDict {
    /// Build from a wordlist.
    #[must_use]
    pub fn new<I: IntoIterator<Item = String>>(words: I) -> Self {
        let words: std::collections::HashSet<String> = words.into_iter().collect();
        let max_chars = words.iter().map(|w| w.chars().count()).max().unwrap_or(1);
        WordDict { words, max_chars }
    }
}

/// Per-char `can_break` (after this char) using dictionary segmentation for Thai/Lao runs and
/// [`break_class`] elsewhere. Thai words are found by longest-match over `dict` — a break is placed at
/// each word boundary. (An unknown char that matches nothing is its own one-char "word".)
#[must_use]
pub fn dict_breaks(text: &str, dict: &WordDict) -> Vec<bool> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut can = vec![false; n];
    let mut i = 0;
    while i < n {
        if is_dict_script(chars[i]) {
            let mut len = 1; // longest word in the dict starting at i (else a single char)
            for l in (2..=dict.max_chars.min(n - i)).rev() {
                if dict
                    .words
                    .contains(&chars[i..i + l].iter().collect::<String>())
                {
                    len = l;
                    break;
                }
            }
            can[i + len - 1] = true; // break after the matched word
            i += len;
        } else {
            can[i] = matches!(break_class(chars[i]), Break::Soft | Break::Mandatory);
            i += 1;
        }
    }
    can
}

/// A laid-out line over the box range `[start, end)`: its natural width and the fit ratio.
///
/// `fit` is the Knuth–Plass adjustment ratio `r = (width − natural) / stretch` — the bulk ratio
/// `x = B/U` of the `(slack, stretch)` pair:
/// - `r == 0` — the line is exact by advance (or has no stretch);
/// - `r > 0`  — stretched to fill (`r` units of stretch per unit stretchability);
/// - `natural > width` — **overfull**: an unbreakable run wider than the box (check `natural`, since
///   with zero stretch `fit` stays 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub natural: f32,
    pub fit: f32,
}

impl Line {
    /// Whether the line's natural width exceeds the target (an unbreakable run too wide to fit).
    #[must_use]
    pub fn is_overfull(&self, width: f32) -> bool {
        self.natural > width
    }
}

/// The prepared ruler — the integer-px cumulative advance (the seed) **plus a coordinate→break
/// index**. Locating a line is then a direct array read by coordinate (`Web.getNode` — "addresses
/// are coordinates, not tallies", `DiscreteArithmetic`), **not** a search: no `log n`. Built **once**
/// by [`Ruler::prepare`] (the certify); every break / fit / viewport query after is **O(1)**.
///
/// Advances are snapped to integer px, which makes the coordinate universe exact (a break at integer
/// `cum` is found by `col_break[floor(target)]` with no quantization error). Crisp px-grid layout;
/// for sub-px, scale advances (e.g. ×64, FreeType's 26.6 fixed point) before building.
pub struct Ruler {
    cum: Vec<u32>,            // cum[i] = Σ px-advance of boxes[0..i]; len = n+1; monotonic
    cum_stretch: Vec<f32>,    // cum_stretch[i] = Σ stretch of boxes[0..i]
    col_break: Vec<u32>, // col_break[x] = box index of the largest break with cum ≤ x px (0 = none)
    col_glyph: Vec<u32>, // col_glyph[x] = the glyph index whose cell contains px column x (hit-test)
    next_break: Vec<u32>, // next_break[i] = smallest break index > i (the overlong-run fallback)
    next_mandatory: Vec<u32>, // next_mandatory[i] = smallest forced-break index > i (caps a line)
    total_px: u32,
}

impl Ruler {
    /// Build the ruler — the `fuse` prefix sum **and** the coordinate→break index, in one pass each.
    /// O(n + total px), done **once** (the certify: you must read each advance once — fontdue's
    /// `append` is O(n) too). After this, every query below is O(1).
    #[must_use]
    pub fn prepare(boxes: &[Box]) -> Ruler {
        let n = boxes.len();
        let mut cum = Vec::with_capacity(n + 1);
        let mut cum_stretch = Vec::with_capacity(n + 1);
        let mut is_break = vec![false; n + 1]; // is_break[j] = a break is allowed before box j
        let mut is_mandatory = vec![false; n + 1]; // is_mandatory[j] = a break is FORCED before box j
        cum.push(0);
        cum_stretch.push(0.0);
        let (mut a, mut s) = (0u32, 0.0f32);
        for (i, b) in boxes.iter().enumerate() {
            a += b.advance.max(0.0).round() as u32;
            s += b.stretch;
            cum.push(a);
            cum_stretch.push(s);
            if b.can_break || b.must_break {
                is_break[i + 1] = true;
            }
            if b.must_break {
                is_mandatory[i + 1] = true;
            }
        }
        is_break[n] = true; // the end of the run is always a break
        let total_px = a;

        // coordinate → break: col_break[x] = the largest break index with cum ≤ x. Cum is monotonic,
        // so a single forward walk fills it (O(total_px + n)) — the index that turns the search into
        // a coordinate read.
        let mut col_break = vec![0u32; total_px as usize + 1];
        let mut best = 0u32;
        let mut bi = 0usize;
        for (x, slot) in col_break.iter_mut().enumerate() {
            while bi <= n && (cum[bi] as usize) <= x {
                if is_break[bi] {
                    best = bi as u32;
                }
                bi += 1;
            }
            *slot = best;
        }
        // coordinate → glyph (the INVERSE read, for hit-testing): col_glyph[x] = the glyph whose cell
        // [cum[g], cum[g+1]) contains px column x. Same forward fill — pixel → glyph in O(1).
        let mut col_glyph = vec![0u32; total_px as usize + 1];
        let mut g = 0usize;
        for (x, slot) in col_glyph.iter_mut().enumerate() {
            while g + 1 <= n && (cum[g + 1] as usize) <= x {
                g += 1;
            }
            *slot = g as u32;
        }
        // next_break[i] / next_mandatory[i] = smallest (forced) break index > i (reverse fill)
        let mut next_break = vec![n as u32; n + 1];
        let mut next_mandatory = vec![n as u32; n + 1];
        let (mut nb, mut nm) = (n as u32, n as u32);
        for i in (0..=n).rev() {
            next_break[i] = nb;
            next_mandatory[i] = nm;
            if is_break[i] {
                nb = i as u32;
            }
            if is_mandatory[i] {
                nm = i as u32;
            }
        }

        Ruler {
            cum,
            cum_stretch,
            col_break,
            col_glyph,
            next_break,
            next_mandatory,
            total_px,
        }
    }

    /// **Hit-test — the inverse coordinate read, O(1)**: the glyph/caret index at pixel `x` measured
    /// from the start of the line beginning at box `line_start`. Pixel → glyph, the same ruler read
    /// backwards. Returns a caret position in `0..=n` (the right edge yields `n` — caret *after* the
    /// last glyph). This is the editing spine: cursor placement, selection, click-to-position — all
    /// fall out here, no search. (Mutation — insert/delete — is the separate O(Δ) ruler patch.)
    #[must_use]
    pub fn glyph_at_x(&self, line_start: usize, x: f32) -> usize {
        let abs = ((self.cum[line_start] as f32) + x.max(0.0)).floor() as usize;
        self.col_glyph[abs.min(self.total_px as usize)] as usize
    }

    /// Number of boxes the ruler covers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cum.len() - 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The farthest break at or after box `start` that fits in `width` — **O(1), a coordinate read**:
    /// index the coordinate→break table at the line's right edge (`cum[start] + width`). No search,
    /// no `log n`. Returns the box index where the line ends (always `> start`; forces the next break
    /// for an overlong run). The random-access primitive: locate one line without touching the rest.
    #[must_use]
    pub fn break_after(&self, start: usize, width: f32) -> usize {
        let target = self.cum[start] as f32 + width.max(0.0);
        let idx = (target.floor() as usize).min(self.total_px as usize);
        let b = self.col_break[idx] as usize; // exact: cum is integer ⇒ cum[b] ≤ floor(target) ⟺ ≤ target
        let soft = if b > start {
            b
        } else {
            self.next_break[start] as usize
        };
        // a mandatory break (\n) ends the line even if it isn't full — and a line can't cross one
        soft.min(self.next_mandatory[start] as usize)
    }

    /// The fit of the line spanning `[start, end)` — **O(1)**, two cumulative lookups: the bulk
    /// ratio `r = (width − natural)/stretch = B/U`.
    #[must_use]
    pub fn line(&self, start: usize, end: usize, width: f32) -> Line {
        let natural = (self.cum[end] - self.cum[start]) as f32;
        let stretch = self.cum_stretch[end] - self.cum_stretch[start];
        let fit = if stretch > 0.0 {
            (width - natural) / stretch
        } else {
            0.0
        };
        Line {
            start,
            end,
            natural,
            fit,
        }
    }

    /// Break into lines for a target `width` — each line is one O(1) [`break_after`] + one O(1)
    /// [`line`]. Materializing **all** L lines is O(L) (output-bound — you emit L records); for
    /// rendering you don't: query the visible lines (each O(1)) and never walk the rest.
    #[must_use]
    pub fn lines(&self, width: f32) -> Vec<Line> {
        let n = self.len();
        let mut out = Vec::new();
        let mut start = 0usize;
        while start < n {
            let end = self.break_after(start, width);
            out.push(self.line(start, end, width));
            start = end;
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// editing — the streaming-law incremental: a document is paragraphs (split on \n), each its OWN
// ruler; an edit re-folds ONLY the changed paragraph (O(Δ), the cerialize incremental face — certify
// once, re-certify just the dirty block). The rest of the document is untouched.
// ─────────────────────────────────────────────────────────────────────────────

/// An editable document: one [`Ruler`] per paragraph. `replace` rebuilds a single paragraph's ruler
/// — **O(paragraph), not O(document)** (the streaming law: re-fold only the dirty block). Reads
/// (lines, hit-test) dispatch per paragraph; with [`Ruler::glyph_at_x`] (the inverse coordinate read)
/// that's the full editing spine — geometry from O(1) reads, mutation from O(Δ) re-folds.
pub struct Document {
    paras: Vec<Ruler>,
}

impl Document {
    /// Build from per-paragraph box runs (the caller `prefill`s each paragraph).
    #[must_use]
    pub fn new(paragraphs: &[Vec<Box>]) -> Self {
        Document {
            paras: paragraphs.iter().map(|b| Ruler::prepare(b)).collect(),
        }
    }
    /// Re-fold paragraph `i` after an edit — **O(paragraph)**, the rest of the document untouched.
    pub fn replace(&mut self, i: usize, boxes: &[Box]) {
        self.paras[i] = Ruler::prepare(boxes);
    }
    /// Insert a new paragraph at `i` (a `\n` split) — O(paragraph) to fold it; the others are moved,
    /// not re-folded.
    pub fn insert(&mut self, i: usize, boxes: &[Box]) {
        self.paras.insert(i, Ruler::prepare(boxes));
    }
    pub fn remove(&mut self, i: usize) {
        self.paras.remove(i);
    }
    #[must_use]
    pub fn paragraph(&self, i: usize) -> &Ruler {
        &self.paras[i]
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.paras.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paras.is_empty()
    }
    /// Total laid-out lines across the document at `width` (each paragraph contributes ≥ 1 line).
    #[must_use]
    pub fn line_count(&self, width: f32) -> usize {
        self.paras.iter().map(|r| r.lines(width).len().max(1)).sum()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// bidi — the permutation applicative ("translation is rotation", Axiom XX): the same logical boxes,
// reordered to VISUAL order. UAX #9 reduced — LTR base, reverse maximal RTL runs. The same shape as
// the universal encoder's orientation change, not a re-layout.
// ─────────────────────────────────────────────────────────────────────────────

/// Writing direction of a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Ltr,
    Rtl,
}

/// Strong direction of a character — Hebrew / Arabic ranges are RTL, everything else LTR. (UAX #9's
/// strong types reduced to ranges, like [`break_class`]; full bidi — weak/neutral resolution, nested
/// levels — is the tabled extension.)
#[must_use]
pub fn char_dir(c: char) -> Dir {
    match c as u32 {
        0x0590..=0x05FF | // Hebrew
        0x0600..=0x06FF | // Arabic
        0x0700..=0x074F | // Syriac
        0x0750..=0x077F | // Arabic Supplement
        0x08A0..=0x08FF | // Arabic Extended-A
        0xFB1D..=0xFDFF | // Hebrew/Arabic presentation forms A
        0xFE70..=0xFEFF => Dir::Rtl, // Arabic presentation forms B
        _ => Dir::Ltr,
    }
}

/// Reorder a logical run to **visual order** — the L2 reordering of UAX #9, reduced to an LTR base:
/// reverse each maximal RTL run in place. Returns `visual`, where `visual[k]` is the logical index of
/// the glyph shown at visual position `k` (so it doubles as the hit-test map, visual ↔ logical). The
/// permutation applicative: same boxes, rotated orientation — feed the reordered boxes to the ruler.
#[must_use]
pub fn bidi_visual_order(dirs: &[Dir]) -> Vec<usize> {
    let n = dirs.len();
    let mut visual: Vec<usize> = (0..n).collect();
    let mut i = 0;
    while i < n {
        if dirs[i] == Dir::Rtl {
            let mut j = i;
            while j < n && dirs[j] == Dir::Rtl {
                j += 1;
            }
            visual[i..j].reverse(); // the run reversal — the permutation
            i = j;
        } else {
            i += 1;
        }
    }
    visual
}

/// Reorder `boxes` to visual order given each box's direction — the applicative applied. Pair with
/// [`bidi_visual_order`] (kept for the hit-test map). Feed the result to [`Ruler::prepare`].
#[must_use]
pub fn bidi_reorder(boxes: &[Box], dirs: &[Dir]) -> Vec<Box> {
    bidi_visual_order(dirs)
        .into_iter()
        .map(|k| boxes[k])
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// fontdue adapter — the real-font applicative (advances from the font, no browser)
// ─────────────────────────────────────────────────────────────────────────────

/// **Prefill** — pretext's `prepare()` stage, browser-free: segment `text`, apply break rules
/// ([`break_class`], UAX #14-lite), and measure each box from the font's advances (the browser-free
/// replacement for `canvas.measureText`). One fold over the codepoint stream → the boxes the ruler
/// folds. No JS, no canvas.
///
/// - **width** from `font.metrics(c).advance_width` (not the browser);
/// - **soft breaks** after spaces / hyphens / between CJK ([`Break::Soft`]); spaces also stretch;
/// - **mandatory breaks** at `\n` and the line/paragraph separators ([`Break::Mandatory`]);
/// - **grapheme clustering, font-driven**: a combining mark has ~0 advance and `Break::None`, so it
///   rides on its base glyph (no width, no break between) without any Unicode-category table.
#[cfg(feature = "fontdue")]
#[must_use]

// ─────────────────────────────────────────────────────────────────────────────
// compositor — the renderer: layout → rasterize (fontdue) → blit. text → pixels, no browser/canvas
// ─────────────────────────────────────────────────────────────────────────────

/// A grayscale framebuffer (0 = white … 255 = black ink) — the "canvas" we own, no browser. The
/// compositor blits fontdue-rasterized glyph coverage into it; glyph **positions come from the ruler**
/// (`cum[i]` is the pen-x of glyph i — an O(1) coordinate read), so a frame paints only the lines you
/// hand it (a viewport): cost ∝ visible pixels, not document size.
#[cfg(feature = "fontdue")]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub pixels: Vec<u8>, // row-major grayscale; 0 = white background, 255 = full ink
}

#[cfg(feature = "fontdue")]
impl Canvas {
    /// A white canvas of `w × h`.
    #[must_use]
    pub fn new(w: usize, h: usize) -> Self {
        Canvas {
            w,
            h,
            pixels: vec![0; w * h],
        }
    }

    /// Alpha-blit a `cw × ch` coverage bitmap with its top-left at `(ox, oy)` — clipped to the canvas
    /// (off-screen rows/cols are skipped, so painting a viewport never touches outside pixels).
    fn blit(&mut self, cov: &[u8], cw: usize, ch: usize, ox: i32, oy: i32) {
        for row in 0..ch {
            let y = oy + row as i32;
            if y < 0 || y as usize >= self.h {
                continue;
            }
            for col in 0..cw {
                let x = ox + col as i32;
                if x < 0 || x as usize >= self.w {
                    continue;
                }
                let c = cov[row * cw + col];
                let p = &mut self.pixels[y as usize * self.w + x as usize];
                *p = (*p).max(c); // ink accumulates (max coverage on overlap)
            }
        }
    }
}

/// Render laid-out `lines` of `chars` into `canvas` — composite each glyph's fontdue raster at the
/// pen position the ruler implies. `chars` is the original text (one char per box, as `prefill`
/// produces); each line is painted at `baseline = i·line_height + ascent`. Paints **only the lines
/// passed** (hand it the viewport, not the whole document) — the per-frame cost is O(visible pixels).
#[cfg(feature = "fontdue")]
pub fn render(
    canvas: &mut Canvas,
    chars: &[char],
    lines: &[Line],
    px: f32,
    line_height: f32,
    ascent: f32,
) {
    for (i, line) in lines.iter().enumerate() {
        let baseline = (i as f32 * line_height + ascent).round() as i32;
        let mut pen = 0.0f32;
        for &c in &chars[line.start..line.end] {
            if c.is_control() {
                continue;
            }
            let (m, cov) = font.rasterize(c, px);
            if m.width > 0 && m.height > 0 {
                let gx = (pen + m.xmin as f32).round() as i32;
                let gy = baseline - m.ymin - m.height as i32; // fontdue: ymin = baseline→bitmap bottom
                canvas.blit(&cov, m.width, m.height, gx, gy);
            }
            pen += m.advance_width;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// font fallback — fontdb's invariant (candidate selection), lean & BYOF: an ordered font set,
// each char drawn from the first font that covers it. Fixes the tofu without a system font DB.
// ─────────────────────────────────────────────────────────────────────────────

/// An ordered set of fonts: the primary first, fallbacks after. Each character is drawn from the
/// **first font that covers it** — fontdb's candidate selection, here over a set you bring (BYOF).
///
/// Selection is **O(1)**, not a per-char scan of the font list: `new` folds every font's cmap into one
/// `char → font index` table **once** (the certify — first font wins), so [`font_index`](Self::font_index)
/// is a single coordinate read regardless of how many fallbacks there are.
#[cfg(feature = "fontdue")]
pub struct FontSet {
    cover: std::collections::HashMap<char, u32>, // char → covering font index (built once)
}

#[cfg(feature = "fontdue")]
impl FontSet {
    /// Build from an ordered list (primary first). Folds all the fonts' cmaps into the cover index.
    #[must_use]
        assert!(!fonts.is_empty(), "FontSet needs at least one font");
        let mut cover = std::collections::HashMap::new();
        for (i, f) in fonts.iter().enumerate() {
            for &c in f.chars().keys() {
                cover.entry(c).or_insert(i as u32); // first font that covers `c` wins
            }
        }
        FontSet { fonts, cover }
    }
    /// The index of the font that covers `c` — **O(1)** (one coordinate read into the cover index),
    /// independent of the number of fonts. Falls back to the primary (0) if nothing covers it.
    #[must_use]
    pub fn font_index(&self, c: char) -> u32 {
        *self.cover.get(&c).unwrap_or(&0)
    }
    /// The font that covers `c` — O(1).
    #[must_use]
        &self.fonts[self.font_index(c) as usize]
    }
    /// Font `i` of the set (for rendering a glyph by its recorded font index).
    #[must_use]
        &self.fonts[i as usize]
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.fonts.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

/// [`prefill`] with fallback: each char's advance comes from the font in `set` that covers it.
#[cfg(feature = "fontdue")]
#[must_use]
pub fn prefill_set(set: &FontSet, text: &str, px: f32) -> Vec<Box> {
    text.chars()
        .map(|c| {
            let advance = set.covering(c).metrics(c, px).advance_width;
            match break_class(c) {
                Break::Mandatory => Box::hard_break(),
                Break::Soft if c == ' ' => Box::space(advance, advance * 0.5),
                Break::Soft => Box {
                    advance,
                    stretch: 0.0,
                    can_break: true,
                    must_break: false,
                },
                Break::None => Box::glyph(advance),
            }
        })
        .collect()
}

/// [`render`] with fallback: each glyph is rasterized from the font in `set` that covers it.
#[cfg(feature = "fontdue")]
pub fn render_set(
    canvas: &mut Canvas,
    set: &FontSet,
    chars: &[char],
    lines: &[Line],
    px: f32,
    line_height: f32,
    ascent: f32,
) {
    for (i, line) in lines.iter().enumerate() {
        let baseline = (i as f32 * line_height + ascent).round() as i32;
        let mut pen = 0.0f32;
        for &c in &chars[line.start..line.end] {
            if c.is_control() {
                continue;
            }
            let (m, cov) = set.covering(c).rasterize(c, px);
            if m.width > 0 && m.height > 0 {
                let gx = (pen + m.xmin as f32).round() as i32;
                let gy = baseline - m.ymin - m.height as i32;
                canvas.blit(&cov, m.width, m.height, gx, gy);
            }
            pen += m.advance_width;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// shaping — rustybuzz (HarfBuzz) as the BYO-shaper applicative (the complex-script SoTA).
// We don't reimplement shaping (per-script syllable tables — the rustybuzz invariant is huge); we
// consume its shaped advances. Kerning, ligatures, and complex scripts flow straight into the fold.
// ─────────────────────────────────────────────────────────────────────────────

/// **Prefill with real shaping** — pretext's `prepare()` over shaped glyphs. Splits on `\n` (shape
/// per run), shapes each run with rustybuzz (kerning / ligatures / Arabic joining / Indic reordering),
/// and emits one [`Box`] per shaped glyph with rustybuzz's advance; break opportunities come from
/// [`break_class`] of the cluster's source char. The layout fold is unchanged — only the box-producing
/// applicative is richer. (Layout/advances are faithful; rendering ligated glyphs by glyph-id is the
/// next plumbing — `Box` would carry the glyph id.)
#[cfg(feature = "shape")]
#[must_use]
pub fn shape_boxes(face: &rustybuzz::Face, text: &str, px: f32) -> Vec<Box> {
    let scale = px / face.units_per_em() as f32;
    let mut boxes = Vec::new();
    for (i, run) in text.split('\n').enumerate() {
        if i > 0 {
            boxes.push(Box::hard_break()); // the mandatory break between runs
        }
        if run.is_empty() {
            continue;
        }
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(run);
        buf.guess_segment_properties(); // detect script/direction (Arabic → RTL + join, etc.)
        let glyphs = rustybuzz::shape(face, &[], buf);
        for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
            let advance = pos.x_advance as f32 * scale; // the SHAPED advance (kerned/ligated)
            let c = run[info.cluster as usize..].chars().next().unwrap_or(' '); // source char
            boxes.push(match break_class(c) {
                Break::Soft if c == ' ' => Box::space(advance, advance * 0.5),
                Break::Soft => Box {
                    advance,
                    stretch: 0.0,
                    can_break: true,
                    must_break: false,
                },
                _ => Box::glyph(advance),
            });
        }
    }
    boxes
}

/// A shaped glyph: the layout box (advance/break) **plus** the font glyph id and pen offsets the
/// shaper produced. `id` indexes the FONT's glyphs (rasterize via `rasterize_indexed`), not a char —
/// so ligatures, Arabic contextual forms, and mark positioning render *faithfully*, not as the
/// per-char isolated glyphs `render` would draw.
#[cfg(feature = "shape")]
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub b: Box,
    pub id: u16,
    /// Index into the `FontSet` this glyph was shaped from (0 for single-font shaping). Lets the
    /// renderer rasterize each glyph from its own font (per-run fallback).
    pub font: u32,
    pub x_off: f32,
    pub y_off: f32,
}

/// [`shape_boxes`] but keeping the glyph **id** and offsets — for faithful rendering of shaped output.
#[cfg(feature = "shape")]
#[must_use]
pub fn shape_glyphs(face: &rustybuzz::Face, text: &str, px: f32) -> Vec<Glyph> {
    let scale = px / face.units_per_em() as f32;
    let mut out = Vec::new();
    for (i, run) in text.split('\n').enumerate() {
        if i > 0 {
            out.push(Glyph {
                b: Box::hard_break(),
                id: 0,
                font: 0,
                x_off: 0.0,
                y_off: 0.0,
            });
        }
        if run.is_empty() {
            continue;
        }
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(run);
        buf.guess_segment_properties(); // detect script/direction (Arabic → RTL + join, etc.)
        let glyphs = rustybuzz::shape(face, &[], buf);
        for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
            let advance = pos.x_advance as f32 * scale;
            let c = run[info.cluster as usize..].chars().next().unwrap_or(' ');
            let b = match break_class(c) {
                Break::Soft if c == ' ' => Box::space(advance, advance * 0.5),
                Break::Soft => Box {
                    advance,
                    stretch: 0.0,
                    can_break: true,
                    must_break: false,
                },
                _ => Box::glyph(advance),
            };
            out.push(Glyph {
                b,
                id: info.glyph_id as u16,
                font: 0,
                x_off: pos.x_offset as f32 * scale,
                y_off: pos.y_offset as f32 * scale,
            });
        }
    }
    out
}

/// **Bidi + shaping composed**: shape each maximal *directional* run in its own direction, so mixed
/// LTR/RTL text comes out in visual order. Strong letters set the run direction; neutrals (space,
/// digits, punctuation) inherit the last strong one; base is LTR. Each RTL run is shaped RTL, so
/// rustybuzz emits its glyphs already reversed — concatenating the runs in logical order gives the
/// visual stream. (Lean UAX #9: single-level, LTR base, last-strong neutral resolution; nested levels
/// / full weak-neutral rules and per-run font fallback are the frontier.)
#[cfg(feature = "shape")]
#[must_use]
pub fn shape_glyphs_bidi(face: &rustybuzz::Face, text: &str, px: f32) -> Vec<Glyph> {
    let scale = px / face.units_per_em() as f32;
    let mut out = Vec::new();
    for (li, line) in text.split('\n').enumerate() {
        if li > 0 {
            out.push(Glyph {
                b: Box::hard_break(),
                id: 0,
                font: 0,
                x_off: 0.0,
                y_off: 0.0,
            });
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            continue;
        }
        // per-char direction: a strong letter sets it; a neutral keeps the last strong (base LTR)
        let mut dirs = vec![Dir::Ltr; chars.len()];
        let mut last = Dir::Ltr;
        for (k, &c) in chars.iter().enumerate() {
            if c.is_alphabetic() {
                last = char_dir(c);
            }
            dirs[k] = last;
        }
        // shape each maximal same-direction run, in logical order
        let mut k = 0;
        while k < chars.len() {
            let d = dirs[k];
            let mut j = k;
            while j < chars.len() && dirs[j] == d {
                j += 1;
            }
            let run: String = chars[k..j].iter().collect();
            let mut buf = rustybuzz::UnicodeBuffer::new();
            buf.push_str(&run);
            buf.guess_segment_properties();
            buf.set_direction(if d == Dir::Rtl {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            });
            let glyphs = rustybuzz::shape(face, &[], buf);
            for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
                let advance = pos.x_advance as f32 * scale;
                let c = run[info.cluster as usize..].chars().next().unwrap_or(' ');
                let b = match break_class(c) {
                    Break::Soft if c == ' ' => Box::space(advance, advance * 0.5),
                    Break::Soft => Box {
                        advance,
                        stretch: 0.0,
                        can_break: true,
                        must_break: false,
                    },
                    _ => Box::glyph(advance),
                };
                out.push(Glyph {
                    b,
                    id: info.glyph_id as u16,
                    font: 0,
                    x_off: pos.x_offset as f32 * scale,
                    y_off: pos.y_offset as f32 * scale,
                });
            }
            k = j;
        }
    }
    out
}

/// The layout boxes of a shaped run (1:1 with the glyphs) — feed to [`Ruler::prepare`].
#[cfg(feature = "shape")]
#[must_use]
pub fn glyph_boxes(glyphs: &[Glyph]) -> Vec<Box> {
    glyphs.iter().map(|g| g.b).collect()
}

/// Render shaped glyphs **by glyph id** — `rasterize_indexed`, not by char. Ligatures and Arabic
/// contextual forms come out as the shaper chose them (the correctness `render` can't give for shaped
/// text). The `font` must be the SAME font the `face` was shaped with (glyph ids are the font's own).
#[cfg(all(feature = "shape", feature = "fontdue"))]
pub fn render_shaped(
    canvas: &mut Canvas,
    glyphs: &[Glyph],
    lines: &[Line],
    px: f32,
    line_height: f32,
    ascent: f32,
) {
    for (i, line) in lines.iter().enumerate() {
        let baseline = (i as f32 * line_height + ascent).round() as i32;
        let mut pen = 0.0f32;
        for g in &glyphs[line.start..line.end] {
            let (m, cov) = font.rasterize_indexed(g.id, px); // by GLYPH ID — faithful to shaping
            if m.width > 0 && m.height > 0 {
                let gx = (pen + g.x_off + m.xmin as f32).round() as i32;
                let gy = baseline - g.y_off.round() as i32 - m.ymin - m.height as i32;
                canvas.blit(&cov, m.width, m.height, gx, gy);
            }
            pen += g.b.advance;
        }
    }
}

/// **Per-run font fallback + bidi + shaping** — the mixed-*script* path. Split the line into maximal
/// runs of the same (direction, covering font) — the font chosen by the **O(1)** cover index — shape
/// each run with its own face (`faces[i]` is the rustybuzz face for `set.font(i)`, same bytes), and tag
/// each glyph with its font. So Latin draws from a Latin font and Arabic from an Arabic font, each
/// shaped correctly, no tofu. Render with [`render_shaped_set`].
#[cfg(all(feature = "shape", feature = "fontdue"))]
#[must_use]
pub fn shape_glyphs_fallback(
    set: &FontSet,
    faces: &[rustybuzz::Face],
    text: &str,
    px: f32,
) -> Vec<Glyph> {
    let mut out = Vec::new();
    for (li, line) in text.split('\n').enumerate() {
        if li > 0 {
            out.push(Glyph {
                b: Box::hard_break(),
                id: 0,
                font: 0,
                x_off: 0.0,
                y_off: 0.0,
            });
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            continue;
        }
        // per char: direction (neutrals inherit last strong, base LTR) + covering font (O(1) read)
        let mut dirs = vec![Dir::Ltr; chars.len()];
        let mut fonts = vec![0u32; chars.len()];
        let mut last = Dir::Ltr;
        for (k, &c) in chars.iter().enumerate() {
            if c.is_alphabetic() {
                last = char_dir(c);
            }
            dirs[k] = last;
            fonts[k] = set.font_index(c);
        }
        for k in 1..chars.len() {
            if !chars[k].is_alphabetic() {
                fonts[k] = fonts[k - 1]; // a neutral joins the surrounding run's font (no split on space)
            }
        }
        // maximal runs of one (direction, font); shape each with its own face
        let mut k = 0;
        while k < chars.len() {
            let (d, ft) = (dirs[k], fonts[k]);
            let mut j = k;
            while j < chars.len() && dirs[j] == d && fonts[j] == ft {
                j += 1;
            }
            let run: String = chars[k..j].iter().collect();
            let face = &faces[ft as usize];
            let scale = px / face.units_per_em() as f32;
            let mut buf = rustybuzz::UnicodeBuffer::new();
            buf.push_str(&run);
            buf.guess_segment_properties();
            buf.set_direction(if d == Dir::Rtl {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            });
            let glyphs = rustybuzz::shape(face, &[], buf);
            for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
                let advance = pos.x_advance as f32 * scale;
                let c = run[info.cluster as usize..].chars().next().unwrap_or(' ');
                let b = match break_class(c) {
                    Break::Soft if c == ' ' => Box::space(advance, advance * 0.5),
                    Break::Soft => Box {
                        advance,
                        stretch: 0.0,
                        can_break: true,
                        must_break: false,
                    },
                    _ => Box::glyph(advance),
                };
                out.push(Glyph {
                    b,
                    id: info.glyph_id as u16,
                    font: ft,
                    x_off: pos.x_offset as f32 * scale,
                    y_off: pos.y_offset as f32 * scale,
                });
            }
            k = j;
        }
    }
    out
}

/// [`render_shaped`] with per-glyph fallback: each glyph rasterized from **its own** font (`g.font`).
#[cfg(all(feature = "shape", feature = "fontdue"))]
pub fn render_shaped_set(
    canvas: &mut Canvas,
    set: &FontSet,
    glyphs: &[Glyph],
    lines: &[Line],
    px: f32,
    line_height: f32,
    ascent: f32,
) {
    for (i, line) in lines.iter().enumerate() {
        let baseline = (i as f32 * line_height + ascent).round() as i32;
        let mut pen = 0.0f32;
        for g in &glyphs[line.start..line.end] {
            let (m, cov) = set.font(g.font).rasterize_indexed(g.id, px);
            if m.width > 0 && m.height > 0 {
                let gx = (pen + g.x_off + m.xmin as f32).round() as i32;
                let gy = baseline - g.y_off.round() as i32 - m.ymin - m.height as i32;
                canvas.blit(&cov, m.width, m.height, gx, gy);
            }
            pen += g.b.advance;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UAX #14 — full line-break opportunities (unicode-linebreak), all classes
// ─────────────────────────────────────────────────────────────────────────────

/// Per-char `(can_break, must_break)` by the **full UAX #14** algorithm (unicode-linebreak) — every
/// break class, not the char-range subset of [`break_class`]. (Thai/Lao **dictionary** breaking is a
/// further layer: UAX #14 alone does not split within those scripts — that needs a wordlist / ICU.)
#[cfg(feature = "uax14")]
#[must_use]
pub fn uax14_breaks(text: &str) -> Vec<(bool, bool)> {
    let opp: std::collections::HashMap<usize, unicode_linebreak::BreakOpportunity> =
        unicode_linebreak::linebreaks(text).collect();
    let mut out = Vec::new();
    let mut bi = 0usize;
    for c in text.chars() {
        bi += c.len_utf8(); // a break before byte `bi` is a break after the char ending there
        out.push(match opp.get(&bi) {
            Some(unicode_linebreak::BreakOpportunity::Mandatory) => (true, true),
            Some(unicode_linebreak::BreakOpportunity::Allowed) => (true, false),
            None => (false, false),
        });
    }
    out
}

/// [`prefill`] using full UAX #14 break opportunities + font advances (browser-free).
#[cfg(all(feature = "uax14", feature = "fontdue"))]
#[must_use]
    text.chars()
        .zip(uax14_breaks(text))
        .map(|(c, (can, must))| {
            let advance = font.metrics(c, px).advance_width;
            if must {
                Box::hard_break()
            } else if can && c == ' ' {
                Box::space(advance, advance * 0.5)
            } else if can {
                Box {
                    advance,
                    stretch: 0.0,
                    can_break: true,
                    must_break: false,
                }
            } else {
                Box::glyph(advance)
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// UAX #9 — full bidi embedding levels (unicode-bidi): weak/neutral/nested resolution
// ─────────────────────────────────────────────────────────────────────────────

/// Per-char embedding levels by the **full UAX #9** algorithm (unicode-bidi) — proper weak (numbers),
/// neutral, and nested-isolate resolution, replacing the lean last-strong of [`char_dir`]. Even level
/// = LTR, odd = RTL.
#[cfg(feature = "uax9")]
#[must_use]
pub fn bidi_levels(text: &str) -> Vec<u8> {
    let info = unicode_bidi::BidiInfo::new(text, None);
    let mut out = Vec::new();
    let mut bi = 0usize;
    for c in text.chars() {
        out.push(info.levels[bi].number()); // levels are per-byte; take the char's first byte
        bi += c.len_utf8();
    }
    out
}

/// Shape with **full UAX #9**, including **nested L2 reordering**: unicode-bidi's `visual_runs` returns
/// the runs of a line already in visual order at every embedding depth (LTR-in-RTL-in-LTR resolved
/// correctly, numbers/neutrals/isolates handled). Each run is shaped in its own direction; emitting
/// them in that visual order is the correct nested result — not just base + single nesting.
#[cfg(all(feature = "uax9", feature = "shape"))]
#[must_use]
pub fn shape_glyphs_uax9(face: &rustybuzz::Face, text: &str, px: f32) -> Vec<Glyph> {
    let scale = px / face.units_per_em() as f32;
    let mut out = Vec::new();
    for (li, line) in text.split('\n').enumerate() {
        if li > 0 {
            out.push(Glyph {
                b: Box::hard_break(),
                id: 0,
                font: 0,
                x_off: 0.0,
                y_off: 0.0,
            });
        }
        if line.is_empty() {
            continue;
        }
        let info = unicode_bidi::BidiInfo::new(line, None);
        let para = &info.paragraphs[0]; // one line ⇒ one paragraph
        let (_, runs) = info.visual_runs(para, para.range.clone()); // byte ranges, VISUAL order
        for run in runs {
            if run.is_empty() {
                continue;
            }
            let rtl = info.levels[run.start].is_rtl();
            let sub = &line[run.clone()];
            let mut buf = rustybuzz::UnicodeBuffer::new();
            buf.push_str(sub);
            buf.guess_segment_properties();
            buf.set_direction(if rtl {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            });
            let glyphs = rustybuzz::shape(face, &[], buf);
            for (gi, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
                let advance = pos.x_advance as f32 * scale;
                let c = sub[gi.cluster as usize..].chars().next().unwrap_or(' ');
                let b = match break_class(c) {
                    Break::Soft if c == ' ' => Box::space(advance, advance * 0.5),
                    Break::Soft => Box {
                        advance,
                        stretch: 0.0,
                        can_break: true,
                        must_break: false,
                    },
                    _ => Box::glyph(advance),
                };
                out.push(Glyph {
                    b,
                    id: gi.glyph_id as u16,
                    font: 0,
                    x_off: pos.x_offset as f32 * scale,
                    y_off: pos.y_offset as f32 * scale,
                });
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// rope — O(log n) char editing + undo (ropey), above the O(Δ) Document re-fold
// ─────────────────────────────────────────────────────────────────────────────

/// A rope-backed editable text buffer (ropey): **O(log n)** char insert/delete and **O(1)** undo
/// snapshots (ropey clones share structure). The mutation layer the editing spine needs above
/// [`Document`]: edit the rope here, re-fold only the affected paragraph's [`Ruler`] (the O(Δ) layout),
/// hit-test with [`Ruler::glyph_at_x`]. (IME's composing region is a UI cursor over this buffer.)
#[cfg(feature = "rope")]
pub struct RopeDocument {
    text: ropey::Rope,
    undo: Vec<ropey::Rope>,
    redo: Vec<ropey::Rope>,
    compose: Option<(usize, usize)>, // IME composing region: (start char, preedit char length)
}

#[cfg(feature = "rope")]
impl RopeDocument {
    #[must_use]
    pub fn new(s: &str) -> Self {
        RopeDocument {
            text: ropey::Rope::from_str(s),
            undo: Vec::new(),
            redo: Vec::new(),
            compose: None,
        }
    }
    fn snapshot(&mut self) {
        self.undo.push(self.text.clone()); // O(1) — ropey clones share the tree
        self.redo.clear();
    }
    /// Insert at a char index — O(log n).
    pub fn insert(&mut self, char_idx: usize, s: &str) {
        self.snapshot();
        self.text.insert(char_idx, s);
    }
    /// Remove a char range — O(log n).
    pub fn remove(&mut self, range: core::ops::Range<usize>) {
        self.snapshot();
        self.text.remove(range);
    }
    /// Undo the last edit — O(1). Returns whether anything was undone.
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(core::mem::replace(&mut self.text, prev));
            true
        } else {
            false
        }
    }
    /// Redo — O(1).
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(core::mem::replace(&mut self.text, next));
            true
        } else {
            false
        }
    }
    #[must_use]
    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }
    #[must_use]
    pub fn text(&self) -> String {
        self.text.to_string()
    }

    // ── IME: a composing region (preedit) over the buffer ─────────────────────────
    // The whole composition is ONE undo entry: `compose_start` snapshots the pre-compose state, the
    // preedit edits go straight to the rope (no per-keystroke undo), `commit` ends it (the snapshot is
    // the undo point), `cancel` removes the preedit and discards the snapshot.

    /// Begin composing at char index `at` (the preedit starts empty). Snapshots for undo.
    pub fn compose_start(&mut self, at: usize) {
        self.snapshot();
        self.compose = Some((at, 0));
    }
    /// Replace the current preedit with `text` (the underlined in-progress text an IME shows).
    pub fn set_compose(&mut self, text: &str) {
        if let Some((start, len)) = self.compose {
            self.text.remove(start..start + len); // drop the old preedit (no new undo entry)
            self.text.insert(start, text);
            self.compose = Some((start, text.chars().count()));
        }
    }
    /// Commit the preedit as final text — the composition becomes one undoable edit.
    pub fn commit_compose(&mut self) {
        self.compose = None;
    }
    /// Cancel: remove the preedit, restore the pre-compose state, no undo entry left behind.
    pub fn cancel_compose(&mut self) {
        if let Some((start, len)) = self.compose.take() {
            self.text.remove(start..start + len);
            self.undo.pop(); // discard the compose_start snapshot — nothing happened
        }
    }
    /// The current composing range `(start, len)` in chars, if a composition is active.
    #[must_use]
    pub fn composing(&self) -> Option<(usize, usize)> {
        self.compose
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// diff — line-level Myers diff, O(ND). Output drives textfold TUI colored display.
// ─────────────────────────────────────────────────────────────────────────────

/// A diff operation on a range of lines.
#[derive(Clone, Debug, PartialEq)]
pub enum DiffOp {
    Equal {
        old: Range<usize>,
        new: Range<usize>,
    },
    Insert {
        new: Range<usize>,
        lines: Vec<String>,
    },
    Delete {
        old: Range<usize>,
        lines: Vec<String>,
    },
    Replace {
        old: Range<usize>,
        new: Range<usize>,
        old_lines: Vec<String>,
        new_lines: Vec<String>,
    },
}

/// A complete diff result: the sequence of ops and both source line sets.
#[derive(Clone, Debug)]
pub struct Diff {
    pub ops: Vec<DiffOp>,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
}

impl Diff {
    /// Number of lines added.
    pub fn added(&self) -> usize {
        self.ops
            .iter()
            .map(|op| match op {
                DiffOp::Insert { new, .. } => new.len(),
                DiffOp::Replace { new, .. } => new.len(),
                _ => 0,
            })
            .sum()
    }
    /// Number of lines removed.
    pub fn removed(&self) -> usize {
        self.ops
            .iter()
            .map(|op| match op {
                DiffOp::Delete { old, .. } => old.len(),
                DiffOp::Replace { old, .. } => old.len(),
                _ => 0,
            })
            .sum()
    }
}

/// Diff two strings line-by-line. Returns the minimal edit sequence (Myers).
pub fn diff(old: &str, new: &str) -> Diff {
    let old_lines: Vec<String> = old.lines().map(|l| l.to_string()).collect();
    let new_lines: Vec<String> = new.lines().map(|l| l.to_string()).collect();
    let ops = myers_diff(&old_lines, &new_lines);
    Diff {
        ops,
        old_lines,
        new_lines,
    }
}

/// Myers diff algorithm: find the shortest edit script (SES) between two line arrays.
/// O(ND) time, O(D²) memory where D = edit distance.
fn myers_diff(a: &[String], b: &[String]) -> Vec<DiffOp> {
    let m = a.len();
    let n = b.len();
    if m == 0 && n == 0 {
        return vec![];
    }
    if m == 0 {
        return vec![DiffOp::Insert {
            new: 0..n,
            lines: b.to_vec(),
        }];
    }
    if n == 0 {
        return vec![DiffOp::Delete {
            old: 0..m,
            lines: a.to_vec(),
        }];
    }
    // Build the shortest edit script via the Myers middle-snake divide and conquer.
    let mut ops = Vec::new();
    build_ops(a, b, 0, m, 0, n, &mut ops);
    ops
}

fn build_ops(
    a: &[String],
    b: &[String],
    a_lo: usize,
    a_hi: usize,
    b_lo: usize,
    b_hi: usize,
    ops: &mut Vec<DiffOp>,
) {
    if a_lo >= a_hi && b_lo >= b_hi {
        return;
    }
    if a_lo >= a_hi {
        ops.push(DiffOp::Insert {
            new: b_lo..b_hi,
            lines: b[b_lo..b_hi].to_vec(),
        });
        return;
    }
    if b_lo >= b_hi {
        ops.push(DiffOp::Delete {
            old: a_lo..a_hi,
            lines: a[a_lo..a_hi].to_vec(),
        });
        return;
    }
    // Find the middle snake
    let max_d = ((a_hi - a_lo) + (b_hi - b_lo) + 1) / 2;
    let max_d2 = 2 * max_d;
    let mid = m_mid(a, b, a_lo, a_hi, b_lo, b_hi, max_d2);
    match mid {
        Some((x, y, u, v)) => {
            build_ops(a, b, a_lo, x, b_lo, y, ops);
            if x < u && y < v {
                // Equal snake
                let len = (u - x).min(v - y);
                ops.push(DiffOp::Equal {
                    old: a_lo + x..a_lo + x + len,
                    new: b_lo + y..b_lo + y + len,
                });
            } else if x < u {
                ops.push(DiffOp::Equal {
                    old: a_lo + x..a_lo + u,
                    new: b_lo + y..b_lo + v,
                });
            }
            build_ops(a, b, u, a_hi, v, b_hi, ops);
        }
        None => {
            ops.push(DiffOp::Replace {
                old: a_lo..a_hi,
                new: b_lo..b_hi,
                old_lines: a[a_lo..a_hi].to_vec(),
                new_lines: b[b_lo..b_hi].to_vec(),
            });
        }
    }
}

fn m_mid(
    a: &[String],
    b: &[String],
    a_lo: usize,
    a_hi: usize,
    b_lo: usize,
    b_hi: usize,
    max_d2: usize,
) -> Option<(usize, usize, usize, usize)> {
    let m = a_hi - a_lo;
    let n = b_hi - b_lo;
    if m == 0 || n == 0 {
        return None;
    }
    // Forward and reverse vectors
    let max_d = (max_d2 + 1) / 2;
    let size = 2 * max_d + 1;
    let off = max_d as isize;
    let mut fwd = vec![-1_isize; size];
    let mut rev = vec![-1_isize; size];
    fwd[(1 + off) as usize] = 0;
    rev[(n as isize - m as isize + off) as usize] = 0;
    let delta = n as isize - m as isize;
    for d in 0..=max_d as isize {
        // Forward
        let f_start = (-d).max(-delta);
        let f_end = d.min(m as isize - delta);
        for k in (f_start..=f_end).step_by(2) {
            let idx = (k + off) as usize;
            let x = if k == -d
                || (k != d && fwd[(k - 1 + off) as usize] < fwd[(k + 1 + off) as usize])
            {
                fwd[(k + 1 + off) as usize]
            } else {
                fwd[(k - 1 + off) as usize] + 1
            };
            let mut y = (x as isize - k) as usize;
            let mut x = x as usize;
            while x < m && y < n && a[a_lo + x] == b[b_lo + y] {
                x += 1;
                y += 1;
            }
            fwd[idx] = x as isize;
            if delta % 2 != 0 && (k - delta).abs() <= d - 1 {
                let rev_idx = ((k - delta) + off) as usize;
                if rev_idx < size && rev[rev_idx] != -1 {
                    return Some((x, y, x, y));
                }
            }
        }
        // Reverse
        let r_start = (-d).max(-(m as isize - delta));
        let r_end = d.min(n as isize + delta);
        for k in (r_start..=r_end).step_by(2) {
            let idx = (k + off) as usize;
            let x = if k == -d
                || (k != d && rev[(k - 1 + off) as usize] > rev[(k + 1 + off) as usize])
            {
                rev[(k + 1 + off) as usize] - 1
            } else {
                rev[(k - 1 + off) as usize]
            };
            let mut x = x as usize;
            let mut y = (x as isize - k) as usize;
            while x > 0 && y > 0 && a[a_lo + x - 1] == b[b_lo + y - 1] {
                x -= 1;
                y -= 1;
            }
            rev[idx] = x as isize;
            if delta % 2 == 0 && (k - delta).abs() <= d {
                let fwd_idx = ((k + delta) + off) as usize;
                if fwd_idx < size && fwd[fwd_idx] != -1 {
                    return Some((x, y, x, y));
                }
            }
        }
    }
    None
}

/// Compute a unified-diff-style string from a Diff.
pub fn format_unified(d: &Diff, ctx: usize) -> String {
    let mut out = String::new();
    let old_off = 0usize;
    let new_off = 0usize;
    let mut i = 0;
    while i < d.ops.len() {
        // Find a hunk: a delete/replace/insert with surrounding context
        if matches!(d.ops[i], DiffOp::Equal { .. }) {
            i += 1;
            continue;
        }
        let hunk_start = i.saturating_sub(ctx);
        let mut hunk_end = (i + 1).min(d.ops.len());
        let mut ctx_after = 0;
        while hunk_end < d.ops.len() && ctx_after < ctx {
            if matches!(d.ops[hunk_end], DiffOp::Equal { .. }) {
                ctx_after += 1;
            } else {
                ctx_after = 0;
            }
            hunk_end += 1;
        }
        // Calculate line offsets for the hunk header
        let mut hunk_old_start = old_off;
        let mut hunk_new_start = new_off;
        for j in hunk_start..hunk_end {
            match &d.ops[j] {
                DiffOp::Equal { old, new } => {
                    if j < i {
                        hunk_old_start += old.len();
                        hunk_new_start += new.len();
                    }
                }
                DiffOp::Delete { old, .. } => {
                    if j < i {
                        hunk_old_start += old.len();
                    }
                }
                DiffOp::Insert { new, .. } => {
                    if j < i {
                        hunk_new_start += new.len();
                    }
                }
                DiffOp::Replace { old, new, .. } => {
                    if j < i {
                        hunk_old_start += old.len();
                        hunk_new_start += new.len();
                    }
                }
            }
        }
        let old_total = d.ops[hunk_start..hunk_end]
            .iter()
            .map(|op| match op {
                DiffOp::Equal { old, .. }
                | DiffOp::Delete { old, .. }
                | DiffOp::Replace { old, .. } => old.len(),
                _ => 0,
            })
            .sum::<usize>();
        let new_total = d.ops[hunk_start..hunk_end]
            .iter()
            .map(|op| match op {
                DiffOp::Equal { new, .. }
                | DiffOp::Insert { new, .. }
                | DiffOp::Replace { new, .. } => new.len(),
                _ => 0,
            })
            .sum::<usize>();
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk_old_start + 1,
            old_total,
            hunk_new_start + 1,
            new_total
        ));
        for j in hunk_start..hunk_end {
            match &d.ops[j] {
                DiffOp::Equal { old, .. } => {
                    for k in old.clone() {
                        out.push_str(&format!(" {}\n", d.old_lines[k]));
                    }
                }
                DiffOp::Delete { lines, .. } => {
                    for l in lines {
                        out.push_str(&format!("-{}\n", l));
                    }
                }
                DiffOp::Insert { lines, .. } => {
                    for l in lines {
                        out.push_str(&format!("+{}\n", l));
                    }
                }
                DiffOp::Replace {
                    old,
                    old_lines,
                    new_lines,
                    ..
                } => {
                    for (_, l) in old.clone().zip(old_lines) {
                        out.push_str(&format!("-{}\n", l));
                    }
                    for l in new_lines {
                        out.push_str(&format!("+{}\n", l));
                    }
                }
            }
        }
        i = hunk_end;
    }
    out
}

use std::ops::Range;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monospace_breaks_at_opportunities() {
        // 10 unit glyphs, each a break opportunity; width 3.5 → lines of 3 (+ a tail of 1).
        let boxes: Vec<Box> = (0..10).map(|_| Box::space(1.0, 0.0)).collect();
        let r = Ruler::prepare(&boxes);
        let lines = r.lines(3.5);
        let ends: Vec<(usize, usize)> = lines.iter().map(|l| (l.start, l.end)).collect();
        assert_eq!(ends, vec![(0, 3), (3, 6), (6, 9), (9, 10)]);
        assert!(lines.iter().all(|l| l.natural <= 3.5));
    }

    #[test]
    fn reflow_reuses_the_same_ruler_no_remeasure() {
        let boxes = [
            Box::glyph(1.0),
            Box::glyph(1.0),
            Box::glyph(1.0),
            Box::space(1.0, 0.5),
            Box::glyph(1.0),
            Box::glyph(1.0),
            Box::glyph(1.0),
        ];
        let r = Ruler::prepare(&boxes); // accumulate ONCE
        assert_eq!(r.lines(4.0).len(), 2); // narrow → breaks at the space
        assert_eq!(r.lines(100.0).len(), 1); // wide → one line; same ruler, no re-accumulate
        assert_eq!(r.lines(4.0).len(), 2); // and back again — pure reads
    }

    #[test]
    fn fit_is_the_bulk_ratio() {
        // natural 8, stretch 4, width 10 → adjustment r = (10−8)/4 = 0.5 = B/U.
        let boxes = [Box {
            advance: 8.0,
            stretch: 4.0,
            can_break: true,
            must_break: false,
        }];
        let r = Ruler::prepare(&boxes);
        let lines = r.lines(10.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].fit, 0.5);
    }

    #[test]
    fn mandatory_break_ends_the_line_early() {
        // "ab\ncd" at a wide width → two lines because of the \n, not the width.
        let boxes = [
            Box::glyph(1.0),
            Box::glyph(1.0),   // ab
            Box::hard_break(), // \n  (box index 2)
            Box::glyph(1.0),
            Box::glyph(1.0), // cd
        ];
        let r = Ruler::prepare(&boxes);
        let lines = r.lines(1000.0); // very wide: only the \n forces the break
        let ends: Vec<(usize, usize)> = lines.iter().map(|l| (l.start, l.end)).collect();
        assert_eq!(ends, vec![(0, 3), (3, 5)]); // line 1 = a,b,\n ; line 2 = c,d
    }

    #[test]
    fn break_classes_cover_the_common_cases() {
        assert_eq!(break_class('\n'), Break::Mandatory);
        assert_eq!(break_class(' '), Break::Soft);
        assert_eq!(break_class('-'), Break::Soft);
        assert_eq!(break_class('\u{00A0}'), Break::None); // NBSP — must NOT break
        assert_eq!(break_class('a'), Break::None);
        assert_eq!(break_class('世'), Break::Soft); // CJK breaks between ideographs
        assert!(is_ideograph('あ') && is_ideograph('한') && !is_ideograph('a'));
    }

    #[test]
    fn cjk_breaks_between_ideographs() {
        // four CJK ideographs (each a soft break), width 2 → two per line.
        let boxes: Vec<Box> = "世界文字"
            .chars()
            .map(|c| match break_class(c) {
                Break::Soft => Box {
                    advance: 1.0,
                    stretch: 0.0,
                    can_break: true,
                    must_break: false,
                },
                _ => Box::glyph(1.0),
            })
            .collect();
        let r = Ruler::prepare(&boxes);
        assert_eq!(r.lines(2.0).len(), 2);
    }

    #[test]
    fn overfull_unbreakable_run_does_not_loop() {
        // a 5-wide unbreakable "word" in a width-3 box: one overfull line, no infinite loop.
        let boxes = [Box::glyph(5.0)];
        let r = Ruler::prepare(&boxes);
        let lines = r.lines(3.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            Line {
                start: 0,
                end: 1,
                natural: 5.0,
                fit: 0.0
            }
        );
        assert!(lines[0].is_overfull(3.0));
    }

    #[test]
    fn empty_is_empty() {
        let r = Ruler::prepare(&[]);
        assert!(r.is_empty());
        assert!(r.lines(10.0).is_empty());
    }

    #[test]
    fn dict_breaks_thai_longest_match() {
        // "สวัสดีโลก" = สวัสดี (6 chars) + โลก (3) with no space between — only the dictionary finds the seam.
        let dict = WordDict::new(["สวัสดี".to_string(), "โลก".to_string()]);
        let can = dict_breaks("สวัสดีโลก", &dict);
        assert_eq!(can.len(), 9);
        assert!(can[5], "break after สวัสดี (longest match)");
        assert!(can[8], "break after โลก");
        assert!(!can[0] && !can[3], "no break inside a word");
    }

    #[cfg(feature = "rope")]
    #[test]
    fn ime_compose_commit_and_cancel() {
        let mut d = RopeDocument::new("ab cd");
        d.compose_start(2); // begin a preedit at index 2
        d.set_compose("X");
        assert_eq!(d.text(), "abX cd");
        d.set_compose("XY"); // refine the preedit (no extra undo entry)
        assert_eq!(d.text(), "abXY cd");
        d.commit_compose();
        assert_eq!(d.composing(), None);
        assert!(d.undo()); // the whole composition is ONE undo
        assert_eq!(d.text(), "ab cd");
        // compose then cancel → no change, no undo entry left
        d.compose_start(2);
        d.set_compose("ZZ");
        assert_eq!(d.text(), "abZZ cd");
        d.cancel_compose();
        assert_eq!(d.text(), "ab cd");
        assert!(!d.undo());
    }

    #[test]
    fn document_edit_refolds_only_one_paragraph() {
        // three paragraphs; editing one replaces only its ruler (O(paragraph), streaming-law incremental).
        let p0 = vec![Box::glyph(1.0), Box::space(1.0, 0.0), Box::glyph(1.0)];
        let p1 = vec![Box::glyph(1.0), Box::glyph(1.0)];
        let p2 = vec![Box::glyph(1.0)];
        let mut doc = Document::new(&[p0, p1, p2]);
        assert_eq!(doc.len(), 3);
        assert_eq!(doc.paragraph(1).len(), 2);
        // edit paragraph 1 → longer; the others are untouched
        doc.replace(
            1,
            &[
                Box::glyph(1.0),
                Box::glyph(1.0),
                Box::glyph(1.0),
                Box::glyph(1.0),
            ],
        );
        assert_eq!(doc.paragraph(1).len(), 4);
        assert_eq!(doc.paragraph(0).len(), 3); // unchanged
        assert_eq!(doc.paragraph(2).len(), 1); // unchanged
        doc.insert(1, &[Box::glyph(1.0)]);
        assert_eq!(doc.len(), 4);
    }

    #[test]
    fn bidi_reverses_rtl_runs_to_visual_order() {
        // logical: L L R R R L  (e.g. "ab" + Hebrew "גדה" + "c") → visual reverses the RTL run.
        let dirs = [Dir::Ltr, Dir::Ltr, Dir::Rtl, Dir::Rtl, Dir::Rtl, Dir::Ltr];
        assert_eq!(bidi_visual_order(&dirs), vec![0, 1, 4, 3, 2, 5]); // the RTL run [2,3,4] reversed
        assert_eq!(char_dir('a'), Dir::Ltr);
        assert_eq!(char_dir('\u{05D0}'), Dir::Rtl); // Hebrew aleph
        assert_eq!(char_dir('\u{0627}'), Dir::Rtl); // Arabic alef
                                                    // the box permutation matches the index map
        let boxes: Vec<Box> = (0..6).map(|i| Box::glyph(i as f32)).collect();
        let visual = bidi_reorder(&boxes, &dirs);
        assert_eq!(
            visual.iter().map(|b| b.advance).collect::<Vec<_>>(),
            vec![0.0, 1.0, 4.0, 3.0, 2.0, 5.0]
        );
    }

    #[test]
    fn hit_test_is_the_inverse_coordinate_read() {
        // 5 glyphs, widths 2,3,1,4,2 → cum = [0,2,5,6,10,12]. Pixel x → glyph (line starts at 0).
        let boxes = [
            Box::glyph(2.0),
            Box::glyph(3.0),
            Box::glyph(1.0),
            Box::glyph(4.0),
            Box::glyph(2.0),
        ];
        let r = Ruler::prepare(&boxes);
        assert_eq!(r.glyph_at_x(0, 0.0), 0); // start of glyph 0
        assert_eq!(r.glyph_at_x(0, 1.9), 0); // inside glyph 0 [0,2)
        assert_eq!(r.glyph_at_x(0, 2.0), 1); // boundary → glyph 1 [2,5)
        assert_eq!(r.glyph_at_x(0, 4.9), 1);
        assert_eq!(r.glyph_at_x(0, 5.0), 2); // glyph 2 [5,6)
        assert_eq!(r.glyph_at_x(0, 11.0), 4); // inside the last glyph [10,12)
        assert_eq!(r.glyph_at_x(0, 999.0), 5); // past the end → caret after the last glyph (position n)
                                               // from a line that starts at glyph 2 (cum 5): x measured from there
        assert_eq!(r.glyph_at_x(2, 0.0), 2);
        assert_eq!(r.glyph_at_x(2, 1.0), 3); // 5+1=6 → glyph 3 [6,10)
    }

    #[cfg(feature = "uax14")]
    #[test]
    fn uax14_full_break_opportunities() {
        // "ab cd" → break only after the space (UAX #14), not after a/b/c; final char ends the text.
        let b = uax14_breaks("ab cd");
        assert_eq!(b.len(), 5);
        assert!(!b[0].0 && !b[1].0); // no break after 'a' or 'b'
        assert!(b[2].0 && !b[2].1); // allowed break after the space
        assert!(!b[3].0); // no break after 'c'
                          // mandatory break is detected: "a\nb" forces it
        let m = uax14_breaks("a\nb");
        assert!(m[1].1); // the \n char is a mandatory break
    }

    #[cfg(feature = "uax9")]
    #[test]
    fn uax9_levels_resolve_direction() {
        assert!(bidi_levels("abc").iter().all(|&l| l % 2 == 0)); // pure LTR → even
        assert!(bidi_levels("\u{05D0}\u{05D1}\u{05D2}")
            .iter()
            .all(|&l| l % 2 == 1)); // pure Hebrew → odd
                                    // a digit run inside Hebrew resolves to an even (LTR) sub-level — the weak rule last-strong misses
        let mixed = bidi_levels("\u{05D0}12\u{05D1}");
        assert_eq!(mixed[0] % 2, 1); // Hebrew letter → RTL
        assert_eq!(mixed[1] % 2, 0); // digit → LTR sub-level (UAX #9 W2/W4)
    }

    #[cfg(feature = "rope")]
    #[test]
    fn rope_edit_undo_redo() {
        let mut d = RopeDocument::new("hello world");
        d.insert(5, " there");
        assert_eq!(d.text(), "hello there world");
        d.remove(0..6); // remove "hello "
        assert_eq!(d.text(), "there world");
        assert!(d.undo()); // back to "hello there world"
        assert_eq!(d.text(), "hello there world");
        assert!(d.undo()); // back to "hello world"
        assert_eq!(d.text(), "hello world");
        assert!(d.redo()); // forward to "hello there world"
        assert_eq!(d.text(), "hello there world");
    }
}
