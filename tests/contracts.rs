//! Contract tests for `ruler`.
//!
//! The O(1) coordinate reads (`break_after`, `glyph_at_x`, `lines`) are checked against naive
//! reference implementations written directly from the documented rules, over pseudo-random
//! box runs. The reference code is deliberately simple and O(n²); it is the oracle, not the
//! product.

use ruler::pdf_base14;
use ruler::{
    bidi_visual_order, break_class, dict_breaks, Box, Break, Dir, Document, Ruler, WordDict,
};

/// xorshift64*: a tiny deterministic PRNG so the tests need no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random box run. Advances are whole px, matching `Ruler::prepare`'s integer snapping.
fn random_boxes(rng: &mut Rng, n: usize) -> Vec<Box> {
    (0..n)
        .map(|_| {
            let adv = rng.below(7) as f32;
            let stretch = [0.0, 0.5, 1.0][rng.below(3) as usize];
            match rng.below(10) {
                0 => Box::hard_break(),
                1..=3 => Box {
                    advance: adv,
                    stretch,
                    can_break: true,
                    must_break: false,
                },
                _ => Box {
                    advance: adv,
                    stretch,
                    can_break: false,
                    must_break: false,
                },
            }
        })
        .collect()
}

/// Cumulative px advances, rounded exactly as `Ruler::prepare` does.
fn naive_cum(boxes: &[Box]) -> Vec<u32> {
    let mut cum = vec![0u32];
    for b in boxes {
        let last = *cum.last().unwrap();
        cum.push(last + b.advance.max(0.0).round() as u32);
    }
    cum
}

/// Naive `break_after`: the farthest break in `(start, n]` whose cumulative advance fits in
/// `width`, else the nearest break after `start`; a forced break caps the line.
fn naive_break_after(boxes: &[Box], start: usize, width: f32) -> usize {
    let n = boxes.len();
    let cum = naive_cum(boxes);
    let total = cum[n] as usize;
    let is_break = |j: usize| j == n || boxes[j - 1].can_break || boxes[j - 1].must_break;
    let idx = ((cum[start] as f32 + width.max(0.0)).floor() as usize).min(total);
    let fitting = (start + 1..=n).rfind(|&b| is_break(b) && cum[b] as usize <= idx);
    let soft = fitting.unwrap_or_else(|| (start + 1..=n).find(|&b| is_break(b)).unwrap());
    let mandatory = (start + 1..=n)
        .find(|&j| boxes[j - 1].must_break)
        .unwrap_or(n);
    soft.min(mandatory)
}

#[test]
fn break_after_matches_naive_reference_on_random_runs() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..400 {
        let n = rng.below(40) as usize;
        let boxes = random_boxes(&mut rng, n);
        let ruler = Ruler::prepare(&boxes);
        for start in 0..n {
            for width in [0.0_f32, 1.0, 2.5, 4.0, 7.0, 13.0, 40.0, 500.0] {
                let got = ruler.break_after(start, width);
                let want = naive_break_after(&boxes, start, width);
                assert_eq!(
                    got, want,
                    "break_after(start={start}, width={width}) on {boxes:?}"
                );
                assert!(got > start, "a line must make progress");
            }
        }
    }
}

#[test]
fn lines_cover_the_run_contiguously_and_report_true_natural_widths() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for _ in 0..300 {
        let n = rng.below(40) as usize;
        let boxes = random_boxes(&mut rng, n);
        let ruler = Ruler::prepare(&boxes);
        let cum = naive_cum(&boxes);
        for width in [3.0_f32, 9.0, 25.0] {
            let lines = ruler.lines(width);
            let mut expect_start = 0;
            for line in &lines {
                assert_eq!(line.start, expect_start, "lines must be contiguous");
                assert!(line.end > line.start);
                assert_eq!(
                    line.natural,
                    (cum[line.end] - cum[line.start]) as f32,
                    "natural width of [{}, {})",
                    line.start,
                    line.end
                );
                expect_start = line.end;
            }
            if n > 0 {
                assert_eq!(expect_start, n, "lines must reach the end of the run");
            } else {
                assert!(lines.is_empty());
            }
        }
    }
}

#[test]
fn glyph_at_x_matches_naive_reference() {
    let mut rng = Rng(0xA076_1D64_78BD_642F);
    for _ in 0..200 {
        let n = rng.below(30) as usize;
        let boxes = random_boxes(&mut rng, n);
        let ruler = Ruler::prepare(&boxes);
        let cum = naive_cum(&boxes);
        let total = cum[n];
        for x in 0..=total + 3 {
            // max{g <= n : cum[g] <= x}: the caret position at pixel x, from the line start.
            let want = (0..=n).rfind(|&g| cum[g] <= x.min(total)).unwrap();
            assert_eq!(ruler.glyph_at_x(0, x as f32), want, "x={x} on {boxes:?}");
        }
    }
}

#[test]
fn ruler_doc_example_breaks_at_the_space_and_reflows_without_remeasuring() {
    let boxes = [
        Box::glyph(1.0),
        Box::glyph(1.0),
        Box::glyph(1.0),
        Box::space(1.0, 0.5),
        Box::glyph(1.0),
        Box::glyph(1.0),
        Box::glyph(1.0),
    ];
    let ruler = Ruler::prepare(&boxes);
    let lines = ruler.lines(4.0);
    assert_eq!(lines.len(), 2);
    assert_eq!((lines[0].start, lines[0].end), (0, 4));
    assert_eq!(ruler.lines(100.0).len(), 1);
}

#[test]
fn fit_ratio_is_slack_over_stretch() {
    // glyph, space(stretch 2), glyph, glyph: one line of natural width 4 in a width of 10.
    let boxes = [
        Box::glyph(1.0),
        Box::space(1.0, 2.0),
        Box::glyph(1.0),
        Box::glyph(1.0),
    ];
    let ruler = Ruler::prepare(&boxes);
    let lines = ruler.lines(10.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].natural, 4.0);
    assert!((lines[0].fit - 3.0).abs() < 1e-6, "fit = (10 - 4) / 2");
}

#[test]
fn unbreakable_run_wider_than_the_box_is_reported_overfull() {
    let boxes = [Box::glyph(10.0), Box::glyph(10.0)];
    let ruler = Ruler::prepare(&boxes);
    let lines = ruler.lines(5.0);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].is_overfull(5.0));
    assert_eq!(lines[0].natural, 20.0);
}

#[test]
fn hard_break_ends_the_line_even_when_it_is_not_full() {
    let boxes = [Box::glyph(1.0), Box::hard_break(), Box::glyph(1.0)];
    let ruler = Ruler::prepare(&boxes);
    let lines = ruler.lines(100.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].end, 2);
}

#[test]
fn document_reflows_per_paragraph_and_counts_lines() {
    let para = |s: &str| -> Vec<Box> { pdf_base14::boxes(s, 1000.0) };
    let mut doc = Document::new(&[para("aa bb"), para("cc")]);
    assert_eq!(doc.len(), 2);
    assert_eq!(doc.line_count(1_000_000.0), 2);
    doc.insert(1, &para("dd ee"));
    assert_eq!(doc.len(), 3);
    doc.replace(0, &para("aa bb cc dd"));
    assert_eq!(doc.paragraph(0).len(), "aa bb cc dd".chars().count());
    doc.remove(2);
    assert_eq!(doc.len(), 2);
    assert!(!doc.is_empty());
}

#[test]
fn base14_helvetica_widths_match_the_afm_table() {
    // Helvetica AFM widths in 1000-unit em space.
    let cases = [
        (' ', 278.0),
        ('0', 556.0),
        ('A', 667.0),
        ('W', 944.0),
        ('a', 556.0),
        ('i', 222.0),
        ('m', 833.0),
        ('w', 722.0),
    ];
    for (c, units) in cases {
        let got = pdf_base14::advance(c, 1000.0);
        assert!((got - units).abs() < 1e-3, "{c:?}: got {got}, want {units}");
    }
    // Scales linearly with font size.
    assert!((pdf_base14::advance('a', 10.0) - 5.56).abs() < 1e-4);
    // Non-ASCII falls back to the default advance (documented).
    assert!((pdf_base14::advance('é', 1000.0) - 556.0).abs() < 1e-3);
    assert!((pdf_base14::text_width("ab", 1000.0) - 1112.0).abs() < 1e-3);
}

#[test]
fn pdf_base14_boxes_mark_spaces_and_newlines() {
    let boxes = pdf_base14::boxes("a b\n", 1000.0);
    assert_eq!(boxes.len(), 4);
    assert!(!boxes[0].can_break);
    assert!(
        boxes[1].can_break && boxes[1].stretch > 0.0,
        "space is breakable and stretchable"
    );
    assert!(boxes[3].must_break, "newline forces a break");
}

#[test]
fn break_class_follows_the_documented_ranges() {
    assert_eq!(break_class('\n'), Break::Mandatory);
    assert_eq!(break_class('\u{2028}'), Break::Mandatory);
    assert_eq!(break_class(' '), Break::Soft);
    assert_eq!(break_class('-'), Break::Soft);
    assert_eq!(break_class('\u{00A0}'), Break::None);
    assert_eq!(break_class('a'), Break::None);
    assert_eq!(break_class('中'), Break::Soft);
}

#[test]
fn dict_breaks_uses_longest_match_for_thai_and_break_class_elsewhere() {
    // Thai ก, ข, ค: the dictionary holds "กข", so a break follows it and another follows ค.
    let dict = WordDict::new(vec!["\u{0E01}\u{0E02}".to_owned()]);
    assert_eq!(
        dict_breaks("\u{0E01}\u{0E02}\u{0E04}", &dict),
        vec![false, true, true]
    );
    assert_eq!(dict_breaks("a b", &dict), vec![false, true, false]);
}

#[test]
fn bidi_visual_order_reverses_each_rtl_run_and_is_a_permutation() {
    use Dir::{Ltr as L, Rtl as R};
    assert_eq!(bidi_visual_order(&[L, L, L]), vec![0, 1, 2]);
    assert_eq!(bidi_visual_order(&[R, R, L]), vec![1, 0, 2]);
    assert_eq!(bidi_visual_order(&[L, R, R, R, L]), vec![0, 3, 2, 1, 4]);

    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for _ in 0..200 {
        let n = rng.below(30) as usize;
        let dirs: Vec<Dir> = (0..n)
            .map(|_| if rng.below(2) == 0 { L } else { R })
            .collect();
        let mut perm = bidi_visual_order(&dirs);
        perm.sort_unstable();
        assert_eq!(perm, (0..n).collect::<Vec<_>>());
    }
}
