use crate::error::FontError;
use super::{Character, Decoder};

const DEFAULT_DPI: f32 = 96.0;
const AA: usize = 4;

#[derive(Debug, Clone, Copy)]
struct Table {
    offset: usize,
    len: usize,
}

#[derive(Debug, Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
    on_curve: bool,
}

#[derive(Debug, Clone)]
struct Outline {
    contours: Vec<Vec<Point>>,
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
}

#[derive(Debug, Clone)]
pub struct Ttf {
    data: Vec<u8>,
    units_per_em: u16,
    index_to_loc_format: i16,
    num_glyphs: u16,
    num_h_metrics: u16,
    ascender: i16,
    descender: i16,
    cmap4: Option<usize>,
    cmap12: Option<usize>,
    loca: Table,
    glyf: Table,
    hmtx: Table,
}

impl Ttf {
    pub fn new(bytes: &[u8]) -> Result<Self, FontError> {
        if bytes.len() < 12 {
            return Err(err("TTF file is too small"));
        }

        let scaler = be_u32(bytes, 0)?;
        if scaler != 0x0001_0000 && scaler != u32::from_be_bytes(*b"true") {
            return Err(err("Input is not a TrueType font"));
        }

        let num_tables = be_u16(bytes, 4)? as usize;
        let directory_end = 12usize
            .checked_add(num_tables.checked_mul(16).ok_or_else(|| err("TTF table directory overflow"))?)
            .ok_or_else(|| err("TTF table directory overflow"))?;
        if directory_end > bytes.len() {
            return Err(err("Truncated TTF table directory"));
        }

        let find_table = |tag: [u8; 4]| -> Result<Table, FontError> {
            for i in 0..num_tables {
                let p = 12 + i * 16;
                if bytes[p..p + 4] == tag {
                    let offset = be_u32(bytes, p + 8)? as usize;
                    let len = be_u32(bytes, p + 12)? as usize;
                    let end = offset.checked_add(len).ok_or_else(|| err("TTF table range overflow"))?;
                    if end > bytes.len() {
                        return Err(err("TTF table extends beyond the file"));
                    }
                    return Ok(Table { offset, len });
                }
            }
            Err(err("Required TTF table is missing"))
        };

        let head = find_table(*b"head")?;
        let maxp = find_table(*b"maxp")?;
        let cmap = find_table(*b"cmap")?;
        let loca = find_table(*b"loca")?;
        let glyf = find_table(*b"glyf")?;
        let hhea = find_table(*b"hhea")?;
        let hmtx = find_table(*b"hmtx")?;

        require_table_len(head, 54, "head")?;
        require_table_len(maxp, 6, "maxp")?;
        require_table_len(hhea, 36, "hhea")?;
        require_table_len(cmap, 4, "cmap")?;

        let units_per_em = be_u16(bytes, head.offset + 18)?;
        if units_per_em == 0 {
            return Err(err("Invalid unitsPerEm value"));
        }
        let index_to_loc_format = be_i16(bytes, head.offset + 50)?;
        if index_to_loc_format != 0 && index_to_loc_format != 1 {
            return Err(err("Unsupported indexToLocFormat"));
        }

        let num_glyphs = be_u16(bytes, maxp.offset + 4)?;
        let ascender = be_i16(bytes, hhea.offset + 4)?;
        let descender = be_i16(bytes, hhea.offset + 6)?;
        let num_h_metrics = be_u16(bytes, hhea.offset + 34)?;
        if num_h_metrics == 0 || num_h_metrics > num_glyphs.max(1) {
            return Err(err("Invalid numberOfHMetrics"));
        }

        let (cmap4, cmap12) = find_cmaps(bytes, cmap)?;
        if cmap4.is_none() && cmap12.is_none() {
            return Err(err("No supported Unicode cmap subtable (format 4 or 12)"));
        }

        let loca_entries = num_glyphs as usize + 1;
        let required_loca = if index_to_loc_format == 0 {
            loca_entries.checked_mul(2)
        } else {
            loca_entries.checked_mul(4)
        }
        .ok_or_else(|| err("loca size overflow"))?;
        if loca.len < required_loca {
            return Err(err("Truncated loca table"));
        }

        let required_hmtx = (num_h_metrics as usize)
            .checked_mul(4)
            .ok_or_else(|| err("hmtx size overflow"))?;
        if hmtx.len < required_hmtx {
            return Err(err("Truncated hmtx table"));
        }

        Ok(Self {
            data: bytes.to_vec(),
            units_per_em,
            index_to_loc_format,
            num_glyphs,
            num_h_metrics,
            ascender,
            descender,
            cmap4,
            cmap12,
            loca,
            glyf,
            hmtx,
        })
    }

    pub fn from_vec(bytes: Vec<u8>) -> Result<Self, FontError> {
        let mut font = Self::new(&bytes)?;
        font.data = bytes;
        Ok(font)
    }

    pub fn character(&self, pt: u16, character: char) -> Result<Character, FontError> {
        self.rasterize_character(pt, character)
    }

    pub fn characters(&self, pt: u16, chars: impl Into<String>) -> Result<Vec<Character>, FontError> {
        chars
            .into()
            .chars()
            .map(|ch| self.rasterize_character(pt, ch))
            .collect()
    }

    fn rasterize_character(&self, pt: u16, ch: char) -> Result<Character, FontError> {
        if pt == 0 {
            return Ok(Character {
                height: 0,
                width: 0,
                pixels: Vec::new(),
                advance_width: 0,
                bearing_x: 0,
                bearing_y: 0,
                line_height: 0,
                baseline: 0,
            });
        }

        let glyph_id = self.glyph_index(ch as u32)?;
        let scale = (pt as f32 * DEFAULT_DPI / 72.0) / self.units_per_em as f32;

        // TrueType positions glyphs with horizontal metrics, not with the
        // cropped bitmap width. Keeping these metrics with the rasterized
        // character is what lets callers produce a stable baseline and normal
        // spacing for fonts such as Liberation Sans.
        let advance_width =
            (self.advance_width(glyph_id)? as f32 * scale).round().max(0.0) as u32;
        let line_height =
            (((self.ascender as i32 - self.descender as i32).max(1) as f32) * scale)
                .ceil()
                .max(1.0) as u32;
        let baseline =
            ((self.ascender.max(0) as f32) * scale).ceil().max(0.0) as u32;

        let outline = self.load_glyph(glyph_id, 0)?;

        if outline.contours.is_empty() {
            // Whitespace and empty glyphs have an advance but no visible ink.
            // Keep a zero-sized bitmap so renderers don't waste time walking a
            // fully transparent line-height-sized rectangle.
            return Ok(Character {
                height: 0,
                width: 0,
                pixels: Vec::new(),
                advance_width,
                bearing_x: 0,
                bearing_y: 0,
                line_height,
                baseline,
            });
        }

        let glyph_w = (outline.x_max as i32 - outline.x_min as i32).max(1) as f32 * scale;
        let glyph_h = (outline.y_max as i32 - outline.y_min as i32).max(1) as f32 * scale;
        let width = glyph_w.ceil().max(1.0) as u32;
        let height = glyph_h.ceil().max(1.0) as u32;

        // The bitmap is cropped to the glyph's ink bounds. These bearings say
        // where that cropped bitmap belongs relative to the pen and baseline.
        let bearing_x = (outline.x_min as f32 * scale).floor() as i32;
        let bearing_y = (outline.y_max as f32 * scale).ceil() as i32;

        let mut flattened: Vec<Vec<(f32, f32)>> = Vec::with_capacity(outline.contours.len());
        for contour in &outline.contours {
            let path = flatten_contour(
                contour,
                scale,
                outline.x_min as f32,
                outline.y_max as f32,
            );
            if path.len() >= 3 {
                flattened.push(path);
            }
        }

        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| err("Character bitmap is too large"))?;
        let mut pixels = vec![0xFFFF_FF00u32; pixel_count];

        for y in 0..height as usize {
            for x in 0..width as usize {
                let mut covered = 0usize;
                for sy in 0..AA {
                    let py = y as f32 + (sy as f32 + 0.5) / AA as f32;
                    for sx in 0..AA {
                        let px = x as f32 + (sx as f32 + 0.5) / AA as f32;
                        if inside_outline(px, py, &flattened) {
                            covered += 1;
                        }
                    }
                }

                if covered != 0 {
                    let alpha = ((covered * 255 + (AA * AA / 2)) / (AA * AA)) as u32;
                    pixels[y * width as usize + x] = 0xFFFF_FF00 | alpha;
                }
            }
        }

        Ok(Character {
            height,
            width,
            pixels,
            advance_width,
            bearing_x,
            bearing_y,
            line_height,
            baseline,
        })
    }

    fn glyph_index(&self, codepoint: u32) -> Result<u16, FontError> {
        if let Some(offset) = self.cmap12 {
            let gid = cmap12_lookup(&self.data, offset, codepoint)?;
            if gid != 0 && gid < self.num_glyphs as u32 {
                return Ok(gid as u16);
            }
        }
        if codepoint <= 0xFFFF {
            if let Some(offset) = self.cmap4 {
                let gid = cmap4_lookup(&self.data, offset, codepoint as u16)?;
                if gid < self.num_glyphs {
                    return Ok(gid);
                }
            }
        }
        Ok(0)
    }

    fn glyph_range(&self, glyph_id: u16) -> Result<(usize, usize), FontError> {
        if glyph_id >= self.num_glyphs {
            return Err(err("Glyph index is out of range"));
        }

        let idx = glyph_id as usize;
        let read_offset = |entry: usize| -> Result<usize, FontError> {
            if self.index_to_loc_format == 0 {
                Ok(be_u16(&self.data, self.loca.offset + entry * 2)? as usize * 2)
            } else {
                Ok(be_u32(&self.data, self.loca.offset + entry * 4)? as usize)
            }
        };

        let start_rel = read_offset(idx)?;
        let end_rel = read_offset(idx + 1)?;
        if end_rel < start_rel || end_rel > self.glyf.len {
            return Err(err("Invalid glyph range in loca table"));
        }
        Ok((self.glyf.offset + start_rel, self.glyf.offset + end_rel))
    }

    fn advance_width(&self, glyph_id: u16) -> Result<u16, FontError> {
        let gid = glyph_id as usize;
        let metrics = self.num_h_metrics as usize;
        let metric_index = gid.min(metrics - 1);
        be_u16(&self.data, self.hmtx.offset + metric_index * 4)
    }

    fn load_glyph(&self, glyph_id: u16, depth: u8) -> Result<Outline, FontError> {
        if depth > 16 {
            return Err(err("Compound glyph recursion is too deep"));
        }

        let (start, end) = self.glyph_range(glyph_id)?;
        if start == end {
            return Ok(Outline {
                contours: Vec::new(),
                x_min: 0,
                y_min: 0,
                x_max: 0,
                y_max: 0,
            });
        }
        if end - start < 10 {
            return Err(err("Truncated glyf record"));
        }

        let number_of_contours = be_i16(&self.data, start)?;
        let x_min = be_i16(&self.data, start + 2)?;
        let y_min = be_i16(&self.data, start + 4)?;
        let x_max = be_i16(&self.data, start + 6)?;
        let y_max = be_i16(&self.data, start + 8)?;

        if number_of_contours >= 0 {
            let contours = self.parse_simple_glyph(start, end, number_of_contours as usize)?;
            Ok(Outline { contours, x_min, y_min, x_max, y_max })
        } else {
            let contours = self.parse_compound_glyph(start, end, depth + 1)?;
            Ok(Outline { contours, x_min, y_min, x_max, y_max })
        }
    }

    fn parse_simple_glyph(
        &self,
        start: usize,
        end: usize,
        contour_count: usize,
    ) -> Result<Vec<Vec<Point>>, FontError> {
        if contour_count == 0 {
            return Ok(Vec::new());
        }

        let mut p = start + 10;
        let end_pts_bytes = contour_count
            .checked_mul(2)
            .ok_or_else(|| err("Glyph contour count overflow"))?;
        if p + end_pts_bytes > end {
            return Err(err("Truncated endPtsOfContours"));
        }

        let mut end_pts = Vec::with_capacity(contour_count);
        for _ in 0..contour_count {
            end_pts.push(be_u16(&self.data, p)? as usize);
            p += 2;
        }
        let point_count = end_pts.last().copied().unwrap_or(0) + 1;

        if p + 2 > end {
            return Err(err("Truncated glyph instructions length"));
        }
        let instruction_len = be_u16(&self.data, p)? as usize;
        p += 2;
        p = p.checked_add(instruction_len).ok_or_else(|| err("Glyph instruction overflow"))?;
        if p > end {
            return Err(err("Truncated glyph instructions"));
        }

        let mut flags = Vec::with_capacity(point_count);
        while flags.len() < point_count {
            if p >= end {
                return Err(err("Truncated glyph flags"));
            }
            let flag = self.data[p];
            p += 1;
            flags.push(flag);
            if flag & 0x08 != 0 {
                if p >= end {
                    return Err(err("Truncated repeated glyph flag"));
                }
                let repeat = self.data[p] as usize;
                p += 1;
                if flags.len() + repeat > point_count {
                    return Err(err("Glyph flag repeat exceeds point count"));
                }
                for _ in 0..repeat {
                    flags.push(flag);
                }
            }
        }

        let mut xs = Vec::with_capacity(point_count);
        let mut x = 0i32;
        for &flag in &flags {
            let delta = if flag & 0x02 != 0 {
                if p >= end {
                    return Err(err("Truncated x coordinate"));
                }
                let v = self.data[p] as i32;
                p += 1;
                if flag & 0x10 != 0 { v } else { -v }
            } else if flag & 0x10 != 0 {
                0
            } else {
                if p + 2 > end {
                    return Err(err("Truncated x coordinate"));
                }
                let v = be_i16(&self.data, p)? as i32;
                p += 2;
                v
            };
            x = x.checked_add(delta).ok_or_else(|| err("Glyph x coordinate overflow"))?;
            xs.push(x as f32);
        }

        let mut ys = Vec::with_capacity(point_count);
        let mut y = 0i32;
        for &flag in &flags {
            let delta = if flag & 0x04 != 0 {
                if p >= end {
                    return Err(err("Truncated y coordinate"));
                }
                let v = self.data[p] as i32;
                p += 1;
                if flag & 0x20 != 0 { v } else { -v }
            } else if flag & 0x20 != 0 {
                0
            } else {
                if p + 2 > end {
                    return Err(err("Truncated y coordinate"));
                }
                let v = be_i16(&self.data, p)? as i32;
                p += 2;
                v
            };
            y = y.checked_add(delta).ok_or_else(|| err("Glyph y coordinate overflow"))?;
            ys.push(y as f32);
        }

        let mut contours = Vec::with_capacity(contour_count);
        let mut first = 0usize;
        for last in end_pts {
            if last < first || last >= point_count {
                return Err(err("Invalid glyph contour endpoint"));
            }
            let mut contour = Vec::with_capacity(last - first + 1);
            for i in first..=last {
                contour.push(Point {
                    x: xs[i],
                    y: ys[i],
                    on_curve: flags[i] & 0x01 != 0,
                });
            }
            contours.push(contour);
            first = last + 1;
        }

        Ok(contours)
    }

    fn parse_compound_glyph(
        &self,
        start: usize,
        end: usize,
        depth: u8,
    ) -> Result<Vec<Vec<Point>>, FontError> {
        const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
        const ARGS_ARE_XY_VALUES: u16 = 0x0002;
        const WE_HAVE_A_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
        const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

        let mut p = start + 10;
        let mut contours: Vec<Vec<Point>> = Vec::new();
        let mut more = true;

        while more {
            if p + 4 > end {
                return Err(err("Truncated compound glyph component"));
            }
            let flags = be_u16(&self.data, p)?;
            let component_gid = be_u16(&self.data, p + 2)?;
            p += 4;

            let args_are_xy = flags & ARGS_ARE_XY_VALUES != 0;
            let (arg1, arg2) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                if p + 4 > end {
                    return Err(err("Truncated compound glyph arguments"));
                }
                let pair = if args_are_xy {
                    (
                        be_i16(&self.data, p)? as i32,
                        be_i16(&self.data, p + 2)? as i32,
                    )
                } else {
                    (
                        be_u16(&self.data, p)? as i32,
                        be_u16(&self.data, p + 2)? as i32,
                    )
                };
                p += 4;
                pair
            } else {
                if p + 2 > end {
                    return Err(err("Truncated compound glyph arguments"));
                }
                let pair = if args_are_xy {
                    (
                        self.data[p] as i8 as i32,
                        self.data[p + 1] as i8 as i32,
                    )
                } else {
                    (self.data[p] as i32, self.data[p + 1] as i32)
                };
                p += 2;
                pair
            };

            let mut xx = 1.0f32;
            let mut xy = 0.0f32;
            let mut yx = 0.0f32;
            let mut yy = 1.0f32;

            if flags & WE_HAVE_A_SCALE != 0 {
                if p + 2 > end {
                    return Err(err("Truncated compound glyph scale"));
                }
                let s = f2dot14(&self.data, p)?;
                p += 2;
                xx = s;
                yy = s;
            } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                if p + 4 > end {
                    return Err(err("Truncated compound glyph scale"));
                }
                xx = f2dot14(&self.data, p)?;
                yy = f2dot14(&self.data, p + 2)?;
                p += 4;
            } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
                if p + 8 > end {
                    return Err(err("Truncated compound glyph transform"));
                }
                xx = f2dot14(&self.data, p)?;
                xy = f2dot14(&self.data, p + 2)?;
                yx = f2dot14(&self.data, p + 4)?;
                yy = f2dot14(&self.data, p + 6)?;
                p += 8;
            }

            let component = self.load_glyph(component_gid, depth)?;
            let mut transformed = component.contours;
            for contour in &mut transformed {
                for point in contour {
                    let x = point.x;
                    let y = point.y;
                    point.x = xx * x + xy * y;
                    point.y = yx * x + yy * y;
                }
            }

            let (dx, dy) = if args_are_xy {
                (arg1 as f32, arg2 as f32)
            } else {
                let parent_points: Vec<Point> = contours.iter().flat_map(|c| c.iter().copied()).collect();
                let component_points: Vec<Point> = transformed.iter().flat_map(|c| c.iter().copied()).collect();
                let parent_index = usize::try_from(arg1).map_err(|_| err("Invalid compound parent point index"))?;
                let component_index = usize::try_from(arg2).map_err(|_| err("Invalid compound component point index"))?;
                let parent = parent_points.get(parent_index).ok_or_else(|| err("Compound parent point index out of range"))?;
                let child = component_points.get(component_index).ok_or_else(|| err("Compound component point index out of range"))?;
                (parent.x - child.x, parent.y - child.y)
            };

            for contour in &mut transformed {
                for point in contour {
                    point.x += dx;
                    point.y += dy;
                }
            }
            contours.extend(transformed);
            more = flags & MORE_COMPONENTS != 0;
        }

        Ok(contours)
    }
}

impl Decoder for Ttf {
    fn characters(&self, pt: u16, chars: impl Into<String>) -> Result<Vec<Character>, FontError> {
        Ttf::characters(self, pt, chars)
    }

    fn character(&self, pt: u16, character: char) -> Result<Character, FontError> {
        Ttf::character(self, pt, character)
    }
}

fn find_cmaps(data: &[u8], cmap: Table) -> Result<(Option<usize>, Option<usize>), FontError> {
    let count = be_u16(data, cmap.offset + 2)? as usize;
    if cmap.len < 4 + count.saturating_mul(8) {
        return Err(err("Truncated cmap encoding records"));
    }

    let mut fmt4 = None;
    let mut fmt12 = None;
    let mut fmt4_score = 0u8;
    let mut fmt12_score = 0u8;

    for i in 0..count {
        let p = cmap.offset + 4 + i * 8;
        let platform = be_u16(data, p)?;
        let encoding = be_u16(data, p + 2)?;
        let rel = be_u32(data, p + 4)? as usize;
        if rel >= cmap.len {
            continue;
        }
        let sub = cmap.offset + rel;
        if sub + 2 > cmap.offset + cmap.len {
            continue;
        }
        let format = be_u16(data, sub)?;
        let score = match (platform, encoding) {
            (3, 10) => 4,
            (0, _) => 3,
            (3, 1) => 2,
            (3, 0) => 1,
            _ => 0,
        };
        match format {
            4 if score > fmt4_score => {
                fmt4 = Some(sub);
                fmt4_score = score;
            }
            12 if score > fmt12_score => {
                fmt12 = Some(sub);
                fmt12_score = score;
            }
            _ => {}
        }
    }

    Ok((fmt4, fmt12))
}

fn cmap4_lookup(data: &[u8], base: usize, code: u16) -> Result<u16, FontError> {
    let length = be_u16(data, base + 2)? as usize;
    let end = base.checked_add(length).ok_or_else(|| err("cmap format 4 length overflow"))?;
    if end > data.len() || length < 16 {
        return Err(err("Truncated cmap format 4 table"));
    }

    let seg_count = be_u16(data, base + 6)? as usize / 2;
    if seg_count == 0 {
        return Ok(0);
    }
    let end_codes = base + 14;
    let start_codes = end_codes + seg_count * 2 + 2;
    let deltas = start_codes + seg_count * 2;
    let range_offsets = deltas + seg_count * 2;
    if range_offsets + seg_count * 2 > end {
        return Err(err("Malformed cmap format 4 table"));
    }

    for i in 0..seg_count {
        let end_code = be_u16(data, end_codes + i * 2)?;
        if code > end_code {
            continue;
        }
        let start_code = be_u16(data, start_codes + i * 2)?;
        if code < start_code {
            return Ok(0);
        }
        let delta = be_i16(data, deltas + i * 2)? as i32;
        let ro_addr = range_offsets + i * 2;
        let range = be_u16(data, ro_addr)? as usize;
        if range == 0 {
            return Ok(((code as i32 + delta) & 0xFFFF) as u16);
        }

        let glyph_addr = ro_addr
            .checked_add(range)
            .and_then(|v| v.checked_add((code as usize - start_code as usize) * 2))
            .ok_or_else(|| err("cmap glyph address overflow"))?;
        if glyph_addr + 2 > end {
            return Ok(0);
        }
        let glyph = be_u16(data, glyph_addr)?;
        if glyph == 0 {
            return Ok(0);
        }
        return Ok(((glyph as i32 + delta) & 0xFFFF) as u16);
    }
    Ok(0)
}

fn cmap12_lookup(data: &[u8], base: usize, code: u32) -> Result<u32, FontError> {
    let length = be_u32(data, base + 4)? as usize;
    let end = base.checked_add(length).ok_or_else(|| err("cmap format 12 length overflow"))?;
    if end > data.len() || length < 16 {
        return Err(err("Truncated cmap format 12 table"));
    }
    let groups = be_u32(data, base + 12)? as usize;
    if base + 16 + groups.saturating_mul(12) > end {
        return Err(err("Malformed cmap format 12 groups"));
    }

    let mut lo = 0usize;
    let mut hi = groups;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let p = base + 16 + mid * 12;
        let start = be_u32(data, p)?;
        let finish = be_u32(data, p + 4)?;
        if code < start {
            hi = mid;
        } else if code > finish {
            lo = mid + 1;
        } else {
            let start_gid = be_u32(data, p + 8)?;
            return Ok(start_gid + (code - start));
        }
    }
    Ok(0)
}

fn flatten_contour(contour: &[Point], scale: f32, x_min: f32, y_max: f32) -> Vec<(f32, f32)> {
    if contour.is_empty() {
        return Vec::new();
    }

    let mut expanded: Vec<Point> = Vec::with_capacity(contour.len() * 2);
    for i in 0..contour.len() {
        let a = contour[i];
        let b = contour[(i + 1) % contour.len()];
        expanded.push(a);
        if !a.on_curve && !b.on_curve {
            expanded.push(Point {
                x: (a.x + b.x) * 0.5,
                y: (a.y + b.y) * 0.5,
                on_curve: true,
            });
        }
    }

    let start_index = expanded.iter().position(|p| p.on_curve).unwrap_or(0);
    expanded.rotate_left(start_index);
    if !expanded[0].on_curve {
        return Vec::new();
    }

    let map = |p: Point| ((p.x - x_min) * scale, (y_max - p.y) * scale);
    let mut path = Vec::new();
    let mut current = expanded[0];
    path.push(map(current));

    let mut i = 1usize;
    while i <= expanded.len() {
        let p = expanded[i % expanded.len()];
        if p.on_curve {
            path.push(map(p));
            current = p;
            i += 1;
        } else {
            let end = expanded[(i + 1) % expanded.len()];
            if !end.on_curve {
                break;
            }
            flatten_quad(current, p, end, scale, &map, &mut path);
            current = end;
            i += 2;
        }
    }

    path
}

fn flatten_quad<F>(
    p0: Point,
    p1: Point,
    p2: Point,
    scale: f32,
    map: &F,
    out: &mut Vec<(f32, f32)>,
) where
    F: Fn(Point) -> (f32, f32),
{
    let curvature = ((p0.x - 2.0 * p1.x + p2.x).abs()
        + (p0.y - 2.0 * p1.y + p2.y).abs())
        * scale;
    let steps = ((curvature.sqrt() * 0.75).ceil() as usize).clamp(2, 32);

    for s in 1..=steps {
        let t = s as f32 / steps as f32;
        let u = 1.0 - t;
        let p = Point {
            x: u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
            y: u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
            on_curve: true,
        };
        out.push(map(p));
    }
}

fn inside_outline(x: f32, y: f32, contours: &[Vec<(f32, f32)>]) -> bool {
    let mut winding = 0i32;
    for contour in contours {
        if contour.len() < 2 {
            continue;
        }
        for i in 0..contour.len() - 1 {
            let a = contour[i];
            let b = contour[i + 1];
            if a.1 <= y {
                if b.1 > y && is_left(a, b, (x, y)) > 0.0 {
                    winding += 1;
                }
            } else if b.1 <= y && is_left(a, b, (x, y)) < 0.0 {
                winding -= 1;
            }
        }
    }
    winding != 0
}

#[inline]
fn is_left(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    (b.0 - a.0) * (p.1 - a.1) - (p.0 - a.0) * (b.1 - a.1)
}

#[inline]
fn require_table_len(table: Table, needed: usize, name: &str) -> Result<(), FontError> {
    if table.len < needed {
        Err(err_owned(format!("Truncated {name} table")))
    } else {
        Ok(())
    }
}

#[inline]
fn be_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let end = offset.checked_add(2).ok_or_else(|| err("Font offset overflow"))?;
    let s = data.get(offset..end).ok_or_else(|| err("Unexpected end of font data"))?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}

#[inline]
fn be_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    Ok(be_u16(data, offset)? as i16)
}

#[inline]
fn be_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let end = offset.checked_add(4).ok_or_else(|| err("Font offset overflow"))?;
    let s = data.get(offset..end).ok_or_else(|| err("Unexpected end of font data"))?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

#[inline]
fn f2dot14(data: &[u8], offset: usize) -> Result<f32, FontError> {
    Ok(be_i16(data, offset)? as f32 / 16384.0)
}

#[inline]
fn err(message: &str) -> FontError {
    FontError::new(message)
}

#[inline]
fn err_owned(message: String) -> FontError {
    FontError::new(&message)
}
