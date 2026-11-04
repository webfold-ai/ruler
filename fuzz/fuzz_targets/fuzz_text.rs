//! Fuzz the text classifiers: raw bytes -> break classes, dictionary breaks, and bidi order.
//!
//! The dictionary is built from the first few whitespace-separated words of the input and capped
//! at 8 characters per word. `dict_breaks` cost grows with the longest dictionary word, and
//! dictionaries are caller data, so the cap keeps this target fast without hiding any logic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ruler::{bidi_reorder, bidi_visual_order, break_class, char_dir, dict_breaks, Box, Dir, WordDict};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    for c in text.chars() {
        let _ = break_class(c);
        let _ = char_dir(c);
    }

    let words = text
        .split_whitespace()
        .take(8)
        .map(|w| w.chars().take(8).collect::<String>());
    let dict = WordDict::new(words);
    let breaks = dict_breaks(&text, &dict);
    assert_eq!(breaks.len(), text.chars().count());

    let dirs: Vec<Dir> = text.chars().map(char_dir).collect();
    let perm = bidi_visual_order(&dirs);
    let mut sorted = perm.clone();
    sorted.sort_unstable();
    assert!(sorted.iter().enumerate().all(|(i, &v)| i == v), "bidi order must be a permutation");

    let boxes: Vec<Box> = text.chars().map(|_| Box::glyph(1.0)).collect();
    assert_eq!(bidi_reorder(&boxes, &dirs).len(), boxes.len());
});
