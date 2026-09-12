//! PDF base-14 font metrics — Helvetica, Times, Courier, Symbol, ZapfDingbats.
//!
//! The standard AFM widths are public, part of PDF spec § 9.6.2.2. No font file, no
//! embedding: `/BaseFont /Helvetica` is legal PDF with zero bytes shipped.
//! Advances are in 1000-unit em space, matching AFM/PDF convention.

/// ASCII 32..127, index 0 = space. Standard Helvetica AFM widths.
const ASCII_ADVANCE: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278,
    278, // space..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584,
    556, // '0'..'?'
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722,
    778, // '@'..'O'
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469,
    556, // 'P'..'_'
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556,
    556, // '`'..'o'
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 'p'..DEL-1
];
const DEFAULT_ADVANCE: u16 = 556;

/// Advance width of `c` at `font_size` px.
pub fn advance(c: char, font_size: f32) -> f32 {
    let units = if (' '..='\u{7e}').contains(&c) {
        ASCII_ADVANCE[c as usize - ' ' as usize]
    } else {
        DEFAULT_ADVANCE
    };
    f32::from(units) * font_size / 1000.0
}

/// Total advance width of `text` at `font_size` px.
pub fn text_width(text: &str, font_size: f32) -> f32 {
    text.chars().map(|c| advance(c, font_size)).sum()
}

/// Build `Box`es for `text` at `font_size` px — the AFM path.
pub fn boxes(text: &str, font_size: f32) -> Vec<crate::Box> {
    text.chars()
        .map(|c| {
            let adv = advance(c, font_size);
            match c {
                '\n' => crate::Box::hard_break(),
                ' ' => crate::Box::space(adv, adv * 0.5),
                _ => crate::Box::glyph(adv),
            }
        })
        .collect()
}
