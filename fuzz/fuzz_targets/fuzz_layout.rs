//! Fuzz the text-to-lines path: raw bytes -> base-14 box run -> `Ruler` -> lines and hit-test.
//!
//! Input layout: byte 0 selects the font size (1..=256 px), bytes 1..3 the line width
//! (0..=4096 px, little endian, 1/16 px steps), the rest is the text (lossy UTF-8).
//!
//! Invariants checked (the module contracts, not just "no panic"):
//! - lines are contiguous, start at 0, end at the run length, and each makes progress;
//! - a line that contains an interior break fits the width (its natural width <= width);
//! - hit-testing stays inside the line's caret range.
//!
//! Font size and width are bounded here on purpose. `Ruler::prepare` allocates memory
//! proportional to the total advance in whole px, so an unbounded caller-supplied advance is
//! a memory-exhaustion risk. That limit is documented; this target keeps the input bounded.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ruler::{pdf_base14, Ruler};

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let font_size = 1.0 + f32::from(data[0]);
    let width = f32::from(u16::from_le_bytes([data[1], data[2]])) / 16.0;
    let text = String::from_utf8_lossy(&data[3..]);

    let boxes = pdf_base14::boxes(&text, font_size);
    let ruler = Ruler::prepare(&boxes);
    let n = boxes.len();
    assert_eq!(ruler.len(), n);

    let lines = ruler.lines(width);
    let mut expect_start = 0;
    for line in &lines {
        assert_eq!(line.start, expect_start, "lines must be contiguous");
        assert!(line.end > line.start, "a line must make progress");
        expect_start = line.end;

        let interior_break = (line.start + 1..line.end)
            .any(|b| boxes[b - 1].can_break || boxes[b - 1].must_break);
        if interior_break {
            assert!(
                line.natural <= width,
                "a line with an interior break must fit: natural {} > width {}",
                line.natural,
                width
            );
        }

        for x in [0.0_f32, width / 2.0, width, 1.0e6] {
            let caret = ruler.glyph_at_x(line.start, x);
            assert!(
                (line.start..=n).contains(&caret),
                "caret {caret} outside [{}, {n}]",
                line.start
            );
        }
    }
    assert_eq!(expect_start, if n == 0 { 0 } else { n }, "lines must reach the end");
});
