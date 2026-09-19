//! TrueType font reading — glyph metrics and outlines, zero dependencies.
//!
//! ruler could previously only measure the PDF base-14 AFM fonts (see
//! `pdf_base14`), which is enough to lay out text destined for a PDF that
//! references those fonts by name, and not enough for anything that has to
//! *draw* the text: no real font file, no outlines, no pixels.
//!
//! This module reads an actual TTF. It answers the two questions a renderer
//! asks — "how wide is this glyph" (`advance`, feeding [`crate::Box`]) and
//! "what shape is it" (`outline`) — leaving rasterization to the caller, since
//! that is a pixel-format decision and ruler is a layout kernel.
//!
//! Scope: the tables needed to lay out and draw Latin text — `head`, `maxp`,
//! `hhea`, `hmtx`, `cmap` (formats 4 and 12), `loca`, `glyf` (simple and
//! composite). Not implemented: CFF/OpenType PostScript outlines (`CFF ` table,
//! i.e. `.otf` with cubic curves), hinting, kerning (`kern`/`GPOS`), and
//! variable-font axes. `parse` reports CFF fonts as unsupported rather than
//! pretending to read them.

use std::collections::HashMap;

/// One point on a glyph contour, in font units.
///
/// TrueType outlines are quadratic B-splines: on-curve points are interpolated,
/// off-curve points are control points. Two consecutive off-curve points imply
/// an on-curve point midway between them, which [`Font::outline`] makes explicit
/// so consumers never have to reconstruct it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub on_curve: bool,
}

/// A closed contour. The last point joins back to the first.
pub type Contour = Vec<Point>;

fn u16at(d: &[u8], o: usize) -> Result<u16, String> {
    d.get(o..o + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| format!("TTF: read past end at {o}"))
}

fn i16at(d: &[u8], o: usize) -> Result<i16, String> {
    u16at(d, o).map(|v| v as i16)
}

fn u32at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("TTF: read past end at {o}"))
}

/// A parsed TrueType font.
pub struct Font {
    data: Vec<u8>,
    tables: HashMap<[u8; 4], (usize, usize)>, // tag -> (offset, length)
    units_per_em: u16,
    num_glyphs: u16,
    num_h_metrics: u16,
    loca_long: bool,
    cmap: HashMap<u32, u16>,
}

impl Font {
    /// Parse a TrueType font from its bytes.
    pub fn parse(data: Vec<u8>) -> Result<Font, String> {
        let tag = u32at(&data, 0)?;
        // 0x00010000 = TrueType outlines; 'true' = legacy Apple; 'ttcf' = collection.
        if tag == 0x7474_6366 {
            return Err("TTF: font collections (.ttc) are not supported".into());
        }
        if tag == 0x4F54_544F {
            return Err("TTF: OpenType/CFF outlines are not supported, only TrueType glyf".into());
        }
        if tag != 0x0001_0000 && tag != 0x7472_7565 {
            return Err(format!("TTF: unrecognised sfnt version {tag:#010x}"));
        }

        let num_tables = u16at(&data, 4)? as usize;
        let mut tables = HashMap::new();
        for i in 0..num_tables {
            let rec = 12 + i * 16;
            let t = data
                .get(rec..rec + 4)
                .ok_or_else(|| format!("TTF: table record {i} truncated"))?;
            let tag = [t[0], t[1], t[2], t[3]];
            let off = u32at(&data, rec + 8)? as usize;
            let len = u32at(&data, rec + 12)? as usize;
            if off > data.len() {
                return Err(format!(
                    "TTF: table {} points past end of file",
                    String::from_utf8_lossy(&tag)
                ));
            }
            tables.insert(tag, (off, len));
        }

        let table = |tag: &[u8; 4]| -> Result<(usize, usize), String> {
            tables
                .get(tag)
                .copied()
                .ok_or_else(|| format!("TTF: missing required table {}", String::from_utf8_lossy(tag)))
        };

        if tables.contains_key(b"CFF ") && !tables.contains_key(b"glyf") {
            return Err("TTF: OpenType/CFF outlines are not supported, only TrueType glyf".into());
        }

        let (head, _) = table(b"head")?;
        let units_per_em = u16at(&data, head + 18)?;
        if units_per_em == 0 {
            return Err("TTF: unitsPerEm is zero".into());
        }
        let loca_long = i16at(&data, head + 50)? == 1;

        let (maxp, _) = table(b"maxp")?;
        let num_glyphs = u16at(&data, maxp + 4)?;

        let (hhea, _) = table(b"hhea")?;
        let num_h_metrics = u16at(&data, hhea + 34)?;

        table(b"hmtx")?;
        table(b"loca")?;
        table(b"glyf")?;

        let (cmap_off, _) = table(b"cmap")?;
        let cmap = parse_cmap(&data, cmap_off)?;

        Ok(Font {
            data,
            tables,
            units_per_em,
            num_glyphs,
            num_h_metrics,
            loca_long,
            cmap,
        })
    }

    /// Design units per em — the scale all font-unit values are expressed in.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn num_glyphs(&self) -> u16 {
        self.num_glyphs
    }

    /// Map a character to a glyph id. Returns 0 (`.notdef`) when unmapped,
    /// which is what a renderer should draw anyway.
    pub fn glyph_id(&self, c: char) -> u16 {
        self.cmap.get(&(c as u32)).copied().unwrap_or(0)
    }

    /// Horizontal advance for a glyph, in font units.
    ///
    /// `hmtx` stores `numberOfHMetrics` full entries; every glyph beyond that
    /// repeats the last advance (monospaced tails are stored once, not N times).
    pub fn advance_units(&self, gid: u16) -> Result<f32, String> {
        let (hmtx, _) = self.tables[b"hmtx"];
        let n = self.num_h_metrics.max(1);
        let idx = gid.min(n - 1) as usize;
        Ok(u16at(&self.data, hmtx + idx * 4)? as f32)
    }

    /// Horizontal advance scaled to a pixel em size.
    pub fn advance(&self, gid: u16, px: f32) -> Result<f32, String> {
        Ok(self.advance_units(gid)? * px / self.units_per_em as f32)
    }

    /// Byte range of a glyph's entry in `glyf`, via `loca`.
    fn glyph_range(&self, gid: u16) -> Result<Option<(usize, usize)>, String> {
        if gid >= self.num_glyphs {
            return Ok(None);
        }
        let (loca, _) = self.tables[b"loca"];
        let (start, end) = if self.loca_long {
            (
                u32at(&self.data, loca + gid as usize * 4)? as usize,
                u32at(&self.data, loca + gid as usize * 4 + 4)? as usize,
            )
        } else {
            (
                u16at(&self.data, loca + gid as usize * 2)? as usize * 2,
                u16at(&self.data, loca + gid as usize * 2 + 2)? as usize * 2,
            )
        };
        // start == end is legal and means "no outline" (e.g. the space glyph).
        if end <= start {
            return Ok(None);
        }
        let (glyf, _) = self.tables[b"glyf"];
        Ok(Some((glyf + start, glyf + end)))
    }

    /// Glyph outline in font units, y-up (the font's own coordinate system).
    ///
    /// Implicit on-curve midpoints between consecutive off-curve points are
    /// inserted, so every returned contour alternates predictably and a consumer
    /// can walk it as a sequence of quadratic segments.
    pub fn outline(&self, gid: u16) -> Result<Vec<Contour>, String> {
        self.outline_depth(gid, 0)
    }

    fn outline_depth(&self, gid: u16, depth: u8) -> Result<Vec<Contour>, String> {
        if depth > 5 {
            return Err("TTF: composite glyph nesting too deep".into());
        }
        let Some((start, _end)) = self.glyph_range(gid)? else {
            return Ok(Vec::new());
        };
        let num_contours = i16at(&self.data, start)?;
        if num_contours >= 0 {
            self.simple_outline(start, num_contours as usize)
        } else {
            self.composite_outline(start, depth)
        }
    }

    fn simple_outline(&self, start: usize, num_contours: usize) -> Result<Vec<Contour>, String> {
        let d = &self.data;
        let mut o = start + 10; // skip numberOfContours + bbox

        let mut end_pts = Vec::with_capacity(num_contours);
        for _ in 0..num_contours {
            end_pts.push(u16at(d, o)? as usize);
            o += 2;
        }
        let n_points = end_pts.last().map(|e| e + 1).unwrap_or(0);

        let instr_len = u16at(d, o)? as usize;
        o += 2 + instr_len; // skip hinting bytecode

        // Flags, run-length encoded via the REPEAT bit.
        const ON_CURVE: u8 = 0x01;
        const X_SHORT: u8 = 0x02;
        const Y_SHORT: u8 = 0x04;
        const REPEAT: u8 = 0x08;
        const X_SAME_OR_POS: u8 = 0x10;
        const Y_SAME_OR_POS: u8 = 0x20;

        let mut flags = Vec::with_capacity(n_points);
        while flags.len() < n_points {
            let f = *d.get(o).ok_or("TTF: flags truncated")?;
            o += 1;
            flags.push(f);
            if f & REPEAT != 0 {
                let r = *d.get(o).ok_or("TTF: repeat count truncated")?;
                o += 1;
                for _ in 0..r {
                    if flags.len() >= n_points {
                        break;
                    }
                    flags.push(f);
                }
            }
        }

        // Coordinates are stored as deltas, x's then y's, each either 1 or 2
        // bytes depending on its flag bits.
        let mut xs = Vec::with_capacity(n_points);
        let mut acc = 0i32;
        for &f in &flags {
            if f & X_SHORT != 0 {
                let v = *d.get(o).ok_or("TTF: x coord truncated")? as i32;
                o += 1;
                acc += if f & X_SAME_OR_POS != 0 { v } else { -v };
            } else if f & X_SAME_OR_POS == 0 {
                acc += i16at(d, o)? as i32;
                o += 2;
            }
            xs.push(acc);
        }
        let mut ys = Vec::with_capacity(n_points);
        acc = 0;
        for &f in &flags {
            if f & Y_SHORT != 0 {
                let v = *d.get(o).ok_or("TTF: y coord truncated")? as i32;
                o += 1;
                acc += if f & Y_SAME_OR_POS != 0 { v } else { -v };
            } else if f & Y_SAME_OR_POS == 0 {
                acc += i16at(d, o)? as i32;
                o += 2;
            }
            ys.push(acc);
        }

        let mut contours = Vec::with_capacity(num_contours);
        let mut first = 0usize;
        for &last in &end_pts {
            if last >= n_points || last < first {
                return Err("TTF: contour end point out of range".into());
            }
            let raw: Vec<Point> = (first..=last)
                .map(|i| Point {
                    x: xs[i] as f32,
                    y: ys[i] as f32,
                    on_curve: flags[i] & ON_CURVE != 0,
                })
                .collect();
            contours.push(insert_implied_points(&raw));
            first = last + 1;
        }
        Ok(contours)
    }

    fn composite_outline(&self, start: usize, depth: u8) -> Result<Vec<Contour>, String> {
        const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
        const ARGS_ARE_XY_VALUES: u16 = 0x0002;
        const WE_HAVE_A_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
        const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

        let d = &self.data;
        let mut o = start + 10;
        let mut out = Vec::new();

        loop {
            let flags = u16at(d, o)?;
            let glyph_index = u16at(d, o + 2)?;
            o += 4;

            let (dx, dy) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                let a = i16at(d, o)? as f32;
                let b = i16at(d, o + 2)? as f32;
                o += 4;
                (a, b)
            } else {
                let a = *d.get(o).ok_or("TTF: composite args truncated")? as i8 as f32;
                let b = *d.get(o + 1).ok_or("TTF: composite args truncated")? as i8 as f32;
                o += 2;
                (a, b)
            };
            // Point-matching placement (ARGS_ARE_XY_VALUES clear) is vanishingly
            // rare and needs the parent's points; treat the args as an offset of
            // zero rather than misplacing the component.
            let (dx, dy) = if flags & ARGS_ARE_XY_VALUES != 0 {
                (dx, dy)
            } else {
                (0.0, 0.0)
            };

            // F2Dot14 fixed point: 2 integer bits, 14 fractional.
            let f2dot14 = |v: i16| v as f32 / 16384.0;
            let (a, b, c, dd) = if flags & WE_HAVE_A_SCALE != 0 {
                let s = f2dot14(i16at(d, o)?);
                o += 2;
                (s, 0.0, 0.0, s)
            } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                let sx = f2dot14(i16at(d, o)?);
                let sy = f2dot14(i16at(d, o + 2)?);
                o += 4;
                (sx, 0.0, 0.0, sy)
            } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
                let a = f2dot14(i16at(d, o)?);
                let b = f2dot14(i16at(d, o + 2)?);
                let c = f2dot14(i16at(d, o + 4)?);
                let dd = f2dot14(i16at(d, o + 6)?);
                o += 8;
                (a, b, c, dd)
            } else {
                (1.0, 0.0, 0.0, 1.0)
            };

            for contour in self.outline_depth(glyph_index, depth + 1)? {
                out.push(
                    contour
                        .into_iter()
                        .map(|p| Point {
                            x: a * p.x + c * p.y + dx,
                            y: b * p.x + dd * p.y + dy,
                            on_curve: p.on_curve,
                        })
                        .collect(),
                );
            }

            if flags & MORE_COMPONENTS == 0 {
                break;
            }
        }
        Ok(out)
    }

    /// Lay out `text` as ruler boxes at `px` em size.
    ///
    /// This is the join between font and layout: advances come from `hmtx`,
    /// break opportunities from ruler's own UAX #14 classifier, so the caller
    /// can hand the result straight to [`crate::Ruler::prepare`].
    pub fn boxes(&self, text: &str, px: f32) -> Result<Vec<crate::Box>, String> {
        let mut out = Vec::with_capacity(text.len());
        for c in text.chars() {
            let adv = self.advance(self.glyph_id(c), px)?;
            out.push(match crate::break_class(c) {
                crate::Break::Mandatory => crate::Box::hard_break(),
                // Only real whitespace stretches under justification. A hyphen
                // or an ideograph is equally a break opportunity, but widening
                // one to fill a line would be a rendering bug, so those stay
                // fixed-width glyphs that merely permit a break after.
                crate::Break::Soft if c.is_whitespace() => crate::Box::space(adv, adv * 0.5),
                crate::Break::Soft => crate::Box {
                    advance: adv,
                    stretch: 0.0,
                    can_break: true,
                    must_break: false,
                },
                crate::Break::None => crate::Box::glyph(adv),
            });
        }
        Ok(out)
    }
}

/// Make implied on-curve points explicit.
///
/// TrueType omits the on-curve point between two consecutive off-curve points
/// because it is always their midpoint. Reconstructing it here means every
/// consumer walks the same regular structure instead of re-deriving this rule.
fn insert_implied_points(raw: &[Point]) -> Contour {
    if raw.len() < 2 {
        return raw.to_vec();
    }
    let mut out = Vec::with_capacity(raw.len() + 4);
    for i in 0..raw.len() {
        let p = raw[i];
        let next = raw[(i + 1) % raw.len()];
        out.push(p);
        if !p.on_curve && !next.on_curve {
            out.push(Point {
                x: (p.x + next.x) / 2.0,
                y: (p.y + next.y) / 2.0,
                on_curve: true,
            });
        }
    }
    out
}

/// Parse `cmap`, preferring a full-Unicode subtable over a BMP-only one.
fn parse_cmap(d: &[u8], cmap: usize) -> Result<HashMap<u32, u16>, String> {
    let n = u16at(d, cmap + 2)? as usize;
    let mut best: Option<(u32, usize)> = None; // (score, subtable offset)
    for i in 0..n {
        let rec = cmap + 4 + i * 8;
        let platform = u16at(d, rec)?;
        let encoding = u16at(d, rec + 2)?;
        let off = cmap + u32at(d, rec + 4)? as usize;
        // Prefer Windows full Unicode (3,10), then Windows BMP (3,1), then any
        // Unicode platform (0,*), then Mac Roman as a last resort.
        let score = match (platform, encoding) {
            (3, 10) => 4,
            (3, 1) => 3,
            (0, _) => 2,
            (1, 0) => 1,
            _ => 0,
        };
        if score > 0 && best.map(|(s, _)| score > s).unwrap_or(true) {
            best = Some((score, off));
        }
    }
    let (_, off) = best.ok_or("TTF: no usable cmap subtable")?;

    let mut map = HashMap::new();
    match u16at(d, off)? {
        4 => {
            let seg_count = u16at(d, off + 6)? as usize / 2;
            let ends = off + 14;
            let starts = ends + seg_count * 2 + 2; // +2 for reservedPad
            let deltas = starts + seg_count * 2;
            let range_offsets = deltas + seg_count * 2;
            for s in 0..seg_count {
                let end = u16at(d, ends + s * 2)?;
                let start = u16at(d, starts + s * 2)?;
                let delta = u16at(d, deltas + s * 2)?;
                let ro = u16at(d, range_offsets + s * 2)?;
                if start > end {
                    continue;
                }
                for c in start..=end {
                    // 0xFFFF terminates the last segment and is not a character.
                    if c == 0xFFFF {
                        continue;
                    }
                    let gid = if ro == 0 {
                        c.wrapping_add(delta)
                    } else {
                        // idRangeOffset is a byte offset from its own slot.
                        let addr =
                            range_offsets + s * 2 + ro as usize + (c - start) as usize * 2;
                        let g = u16at(d, addr)?;
                        if g == 0 {
                            continue;
                        }
                        g.wrapping_add(delta)
                    };
                    if gid != 0 {
                        map.insert(c as u32, gid);
                    }
                }
            }
        }
        12 => {
            let n_groups = u32at(d, off + 12)? as usize;
            for g in 0..n_groups {
                let rec = off + 16 + g * 12;
                let start = u32at(d, rec)?;
                let end = u32at(d, rec + 4)?;
                let start_gid = u32at(d, rec + 8)?;
                if start > end || end - start > 0x10_FFFF {
                    continue;
                }
                for c in start..=end {
                    let gid = start_gid + (c - start);
                    if gid != 0 && gid <= u16::MAX as u32 {
                        map.insert(c, gid as u16);
                    }
                }
            }
        }
        other => return Err(format!("TTF: unsupported cmap subtable format {other}")),
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font that is present on essentially every Linux box. Tests that need a
    /// real font skip rather than fail when it is absent, so the suite stays
    /// green on machines without it.
    fn dejavu() -> Option<Font> {
        let path = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
        let data = std::fs::read(path).ok()?;
        Some(Font::parse(data).expect("DejaVuSans should parse"))
    }

    #[test]
    fn parses_a_real_font() {
        let Some(f) = dejavu() else {
            eprintln!("skip: DejaVuSans.ttf not installed");
            return;
        };
        assert_eq!(f.units_per_em(), 2048, "DejaVu is a 2048 upem font");
        assert!(f.num_glyphs() > 1000);
    }

    #[test]
    fn maps_characters_to_glyphs() {
        let Some(f) = dejavu() else { return };
        let a = f.glyph_id('A');
        let b = f.glyph_id('B');
        assert!(a != 0 && b != 0, "A and B must map");
        assert_ne!(a, b);
        // An unmapped codepoint falls back to .notdef rather than panicking.
        assert_eq!(f.glyph_id('\u{10FFFD}'), 0);
    }

    #[test]
    fn advances_are_sane_and_scale_linearly() {
        let Some(f) = dejavu() else { return };
        let i = f.advance(f.glyph_id('i'), 100.0).unwrap();
        let m = f.advance(f.glyph_id('M'), 100.0).unwrap();
        assert!(i > 0.0 && m > 0.0);
        assert!(m > i, "M must be wider than i in a proportional font");
        let m200 = f.advance(f.glyph_id('M'), 200.0).unwrap();
        assert!((m200 - m * 2.0).abs() < 0.01, "advance must scale with em size");
    }

    #[test]
    fn outlines_have_contours_and_bounded_points() {
        let Some(f) = dejavu() else { return };
        let o = f.outline(f.glyph_id('o')).unwrap();
        assert_eq!(o.len(), 2, "lowercase o is an outer and an inner contour");
        for c in &o {
            assert!(c.len() >= 3);
            for p in c {
                assert!(p.x.abs() < 10_000.0 && p.y.abs() < 10_000.0, "wild point {p:?}");
            }
        }
        // A space has an advance but no outline at all.
        assert!(f.outline(f.glyph_id(' ')).unwrap().is_empty());
    }

    #[test]
    fn no_two_consecutive_off_curve_points_remain() {
        let Some(f) = dejavu() else { return };
        for ch in "aoeSB8@&".chars() {
            for c in f.outline(f.glyph_id(ch)).unwrap() {
                for w in c.windows(2) {
                    assert!(
                        w[0].on_curve || w[1].on_curve,
                        "implied midpoint missing in '{ch}'"
                    );
                }
            }
        }
    }

    #[test]
    fn composite_glyphs_resolve_to_real_outlines() {
        let Some(f) = dejavu() else { return };
        // 'é' is normally a composite of 'e' plus an acute accent.
        let acc = f.outline(f.glyph_id('é')).unwrap();
        let plain = f.outline(f.glyph_id('e')).unwrap();
        assert!(
            acc.len() > plain.len(),
            "composite should contribute extra contours"
        );
    }

    #[test]
    fn boxes_feed_the_ruler() {
        let Some(f) = dejavu() else { return };
        let boxes = f.boxes("hello world", 16.0).unwrap();
        assert_eq!(boxes.len(), 11);
        assert!(boxes[5].can_break, "the space must be a break opportunity");
        let r = crate::Ruler::prepare(&boxes);
        let total: f32 = boxes.iter().map(|b| b.advance).sum();
        assert!(total > 0.0);
        assert_eq!(r.len(), 11);
    }

    #[test]
    fn rejects_non_truetype_input() {
        assert!(Font::parse(vec![0; 12]).is_err());
        assert!(Font::parse(b"OTTO________".to_vec()).is_err());
    }
}
