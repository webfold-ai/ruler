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


// Terminal I/O (crossterm/image/terminal/tui) is termfold's concern, not this text-layout
// kernel's — those module files are not part of ruler and their declarations were left dangling
// by a mid-migration commit, which made the crate uncompilable. Removed. ratatui_compat likewise
// lived on the missing tui module; it returns with tui if terminal support is ever folded back in.
//
// pdf-base14 AFM font metrics ARE ruler's (webfold/pdfcn/biota consume `ruler::pdf_base14`); the
// file shipped but was never wired into the crate root. Declared here.
pub mod pdf_base14;

/// Re-exports the core layout types.
pub mod prelude {
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
