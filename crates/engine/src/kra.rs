//! Clean-room Krita (`.kra`) document reader and writer with full animation interoperability.
//!
//! Supports Krita 5.x `.kra` ZIP archives:
//! - Uncompressed `mimetype` (`application/x-krita`) as the first ZIP entry.
//! - `maindoc.xml` with `<IMAGE>`, nested `<layers>` (paint layers & group layers, blend modes,
//!   opacity, visibility, lock, alpha-lock), and `<animation>` (`<framerate>`, `<range>`,
//!   `<currentTime>`).
//! - `documentinfo.xml`, `mergedimage.png`, and `preview.png`.
//! - Krita v2 tile files (`VERSION 2`, 64×64 planar BGRA tiles, reading both raw flag `0` and
//!   LZF-compressed flag `1` tiles, writing raw flag `0` tiles inside deflated ZIP entries).
//! - Per-layer animation `<keyframes>` XML (`<channel name="content">`) mapping Krita frame
//!   files (`layerN.fK`) directly to and from DreamsCraft multi-track [`Timeline`] `CelClip`s,
//!   holds, empty-frame gaps, and static tracks.

use std::collections::HashMap;

use crate::document::{BlendMode, Document, Layer, LayerId, LayerKind, MAX_CANVAS_EDGE};
use crate::native::LayerPixels;
use crate::timeline::{Cel, Clip, ClipId, ClipKind, Timeline, Track, TrackId};
use crate::zip::{ZipReader, ZipWriter};

/// Krita `.kra` file extension.
pub const EXTENSION: &str = "kra";
/// Krita `.kra` MIME type.
pub const MIME: &str = "application/x-krita";

const TILE_SIDE: usize = 64;
const TILE_PIXELS: usize = TILE_SIDE * TILE_SIDE;
const TILE_BYTES: usize = TILE_PIXELS * 4;
const MAX_LAYERS: usize = 2048;

/// Options controlling `.kra` export.
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    /// Export DreamsCraft Timeline / Animation Assist frames as Krita animated layers.
    pub export_animation: bool,
    /// Include `mergedimage.png` and `preview.png` inside the `.kra` archive.
    pub include_preview: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { export_animation: true, include_preview: true }
    }
}

/// What a `.kra` export writes.
pub struct Export<'a> {
    pub document: &'a Document,
    pub pixels: &'a dyn LayerPixels,
    pub merged_png: &'a [u8],
    pub preview_png: &'a [u8],
    pub name: &'a str,
    pub author: &'a str,
    pub options: ExportOptions,
}

/// An artwork imported from a Krita `.kra` file.
pub struct Imported {
    pub document: Document,
    /// Premultiplied RGBA8 pixels per paint layer.
    pub pixels: Vec<(LayerId, Vec<u8>)>,
    pub name: String,
    pub author: String,
}

/// Why a `.kra` file could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    NotAKra,
    Damaged(&'static str),
    TooLarge,
}

/// Writes a `.kra` document into a `Vec<u8>`.
pub fn write(e: &Export<'_>) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    write_into(e, &mut zip);
    zip.finish()
}

/// Streams a `.kra` document into `out`.
pub fn write_to<W: std::io::Write>(e: &Export<'_>, out: W) -> std::io::Result<W> {
    let mut zip = ZipWriter::to(out);
    write_into(e, &mut zip);
    zip.finish_to()
}

fn sanitize_doc_folder(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .take(64)
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." { "DreamsCraft".to_owned() } else { cleaned }
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\r' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn blend_to_krita(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Darken => "darken",
        BlendMode::ColorBurn => "burn",
        BlendMode::LinearBurn => "linear_burn",
        BlendMode::DarkerColor => "darker color",
        BlendMode::Lighten => "lighten",
        BlendMode::Screen => "screen",
        BlendMode::ColorDodge => "dodge",
        BlendMode::Add => "add",
        BlendMode::LighterColor => "lighter color",
        BlendMode::Overlay => "overlay",
        BlendMode::SoftLight => "soft_light_svg",
        BlendMode::HardLight => "hard_light",
        BlendMode::VividLight => "vivid_light",
        BlendMode::LinearLight => "linear_light",
        BlendMode::PinLight => "pin_light",
        BlendMode::HardMix => "hard_mix_photoshop",
        BlendMode::Difference => "diff",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Subtract => "subtract",
        BlendMode::Divide => "divide",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminize",
        BlendMode::Shade => "multiply",
    }
}

fn blend_from_krita(op: &str) -> BlendMode {
    match op.trim().to_ascii_lowercase().as_str() {
        "multiply" => BlendMode::Multiply,
        "darken" => BlendMode::Darken,
        "burn" | "color_burn" => BlendMode::ColorBurn,
        "linear_burn" => BlendMode::LinearBurn,
        "darker color" | "darker_color" => BlendMode::DarkerColor,
        "lighten" => BlendMode::Lighten,
        "screen" => BlendMode::Screen,
        "dodge" | "color_dodge" => BlendMode::ColorDodge,
        "add" | "linear_dodge" => BlendMode::Add,
        "lighter color" | "lighter_color" => BlendMode::LighterColor,
        "overlay" => BlendMode::Overlay,
        "soft_light" | "soft_light_svg" | "soft_light_ifs_illusions" | "soft_light_pegtop_delphi" => {
            BlendMode::SoftLight
        }
        "hard_light" => BlendMode::HardLight,
        "vivid_light" => BlendMode::VividLight,
        "linear_light" => BlendMode::LinearLight,
        "pin_light" => BlendMode::PinLight,
        "hard_mix" | "hard_mix_photoshop" | "hard_mix_softer_photoshop" => BlendMode::HardMix,
        "diff" | "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "subtract" => BlendMode::Subtract,
        "divide" => BlendMode::Divide,
        "hue" | "hue_hsv" | "hue_hsl" => BlendMode::Hue,
        "saturation" | "saturation_hsv" | "saturation_hsl" => BlendMode::Saturation,
        "color" | "color_hsi" | "color_hsl" => BlendMode::Color,
        "luminize" | "luminosity" | "luminosity_sai" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

/// Converts premultiplied RGBA8 pixels to straight (unpremultiplied) BGRA8 planar Krita v2 tiles.
fn encode_kra_v2_tiles(premul_rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || premul_rgba.len() < w.saturating_mul(h).saturating_mul(4) {
        return b"VERSION 2\nTILEWIDTH 64\nTILEHEIGHT 64\nPIXELSIZE 4\nDATA 0\n".to_vec();
    }
    let cols = w.div_ceil(TILE_SIDE);
    let rows = h.div_ceil(TILE_SIDE);
    let mut non_empty: Vec<(usize, usize, Vec<u8>)> = Vec::new();

    for ty in 0..rows {
        let y0 = ty * TILE_SIDE;
        let y1 = (y0 + TILE_SIDE).min(h);
        for tx in 0..cols {
            let x0 = tx * TILE_SIDE;
            let x1 = (x0 + TILE_SIDE).min(w);
            let mut has_alpha = false;
            let mut planar = vec![0u8; TILE_BYTES];
            for y in y0..y1 {
                let ly = y - y0;
                let src_row_base = y * w;
                let dst_row_base = ly * TILE_SIDE;
                for x in x0..x1 {
                    let lx = x - x0;
                    let src_idx = (src_row_base + x) * 4;
                    let a = premul_rgba.get(src_idx + 3).copied().unwrap_or(0);
                    if a == 0 {
                        continue;
                    }
                    has_alpha = true;
                    let pr = premul_rgba.get(src_idx).copied().unwrap_or(0);
                    let pg = premul_rgba.get(src_idx + 1).copied().unwrap_or(0);
                    let pb = premul_rgba.get(src_idx + 2).copied().unwrap_or(0);
                    let (r, g, b) = if a == 255 {
                        (pr, pg, pb)
                    } else {
                        let a16 = u16::from(a);
                        let half = a16 / 2;
                        let un = |c: u8| ((u16::from(c) * 255 + half) / a16).min(255) as u8;
                        (un(pr), un(pg), un(pb))
                    };
                    let p = dst_row_base + lx;
                    // Krita 8-bit RGBA stores pixels as planar BGRA: B, G, R, A.
                    if let Some(slot) = planar.get_mut(p) {
                        *slot = b;
                    }
                    if let Some(slot) = planar.get_mut(TILE_PIXELS + p) {
                        *slot = g;
                    }
                    if let Some(slot) = planar.get_mut(TILE_PIXELS * 2 + p) {
                        *slot = r;
                    }
                    if let Some(slot) = planar.get_mut(TILE_PIXELS * 3 + p) {
                        *slot = a;
                    }
                }
            }
            if has_alpha {
                non_empty.push((x0, y0, planar));
            }
        }
    }

    let mut out = Vec::with_capacity(64 + non_empty.len() * (TILE_BYTES + 24));
    out.extend_from_slice(
        format!("VERSION 2\nTILEWIDTH {TILE_SIDE}\nTILEHEIGHT {TILE_SIDE}\nPIXELSIZE 4\nDATA {}\n", non_empty.len())
            .as_bytes(),
    );
    for (left, top, planar) in non_empty {
        let len_with_flag = 1 + planar.len();
        out.extend_from_slice(format!("{left},{top},LZF,{len_with_flag}\n").as_bytes());
        out.push(0u8); // flag 0: uncompressed planar BGRA (outer ZIP entry is deflated)
        out.extend_from_slice(&planar);
    }
    out
}

fn empty_kra_v2_tiles() -> &'static [u8] {
    b"VERSION 2\nTILEWIDTH 64\nTILEHEIGHT 64\nPIXELSIZE 4\nDATA 0\n"
}

/// Clean-room LZF decompressor for Krita v2 tile payloads (flag = 1).
fn lzf_decompress(input: &[u8], expected_len: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(expected_len);
    let mut i = 0usize;
    while i < input.len() {
        let ctrl = usize::from(*input.get(i)?);
        i += 1;
        if ctrl < 32 {
            // Literal run of (ctrl + 1) bytes
            let run = ctrl + 1;
            let end = i.checked_add(run)?;
            let slice = input.get(i..end)?;
            if out.len().checked_add(run)? > expected_len {
                return None;
            }
            out.extend_from_slice(slice);
            i = end;
        } else {
            // Back-reference
            let mut len = ctrl >> 5;
            let mut offset = (ctrl & 0x1F) << 8;
            if len == 7 {
                len = len.checked_add(usize::from(*input.get(i)?))?;
                i += 1;
            }
            offset |= usize::from(*input.get(i)?);
            i += 1;
            len += 2;
            let back = offset + 1;
            if back > out.len() || out.len().checked_add(len)? > expected_len {
                return None;
            }
            for _ in 0..len {
                let src = out.len() - back;
                let byte = *out.get(src)?;
                out.push(byte);
            }
        }
    }
    (out.len() == expected_len).then_some(out)
}

/// Decodes a Krita v2 tile file into premultiplied RGBA8 canvas pixels (`width × height × 4`).
fn decode_kra_v2_tiles(data: &[u8], width: u32, height: u32, layer_x: i32, layer_y: i32) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let total = w.checked_mul(h)?.checked_mul(4)?;
    let mut out = vec![0u8; total];

    let mut pos = 0usize;
    let read_line = |buf: &[u8], cursor: &mut usize| -> Option<String> {
        let start = *cursor;
        if start >= buf.len() {
            return None;
        }
        let mut end = start;
        while end < buf.len() && buf.get(end).copied() != Some(b'\n') {
            end += 1;
        }
        let line = std::str::from_utf8(buf.get(start..end)?).ok()?.trim_end_matches('\r').to_owned();
        *cursor = if end < buf.len() { end + 1 } else { end };
        Some(line)
    };

    let mut tile_w = TILE_SIDE;
    let mut tile_h = TILE_SIDE;
    let mut pixel_size = 4usize;
    let mut tile_count = 0usize;

    // Parse header lines up to DATA <count>
    for _ in 0..16 {
        let line = read_line(data, &mut pos)?;
        if let Some(rest) = line.strip_prefix("TILEWIDTH ") {
            tile_w = rest.trim().parse::<usize>().ok()?.clamp(1, 512);
        } else if let Some(rest) = line.strip_prefix("TILEHEIGHT ") {
            tile_h = rest.trim().parse::<usize>().ok()?.clamp(1, 512);
        } else if let Some(rest) = line.strip_prefix("PIXELSIZE ") {
            pixel_size = rest.trim().parse::<usize>().ok()?;
        } else if let Some(rest) = line.strip_prefix("DATA ") {
            tile_count = rest.trim().parse::<usize>().ok()?;
            break;
        }
    }

    if pixel_size != 4 && pixel_size != 8 && pixel_size != 1 && pixel_size != 2 {
        return None;
    }
    let tile_pixels = tile_w.checked_mul(tile_h)?;
    let expected_tile_bytes = tile_pixels.checked_mul(pixel_size)?;

    for _ in 0..tile_count {
        let header = read_line(data, &mut pos)?;
        let mut parts = header.split(',');
        let left = parts.next()?.trim().parse::<i32>().ok()?;
        let top = parts.next()?.trim().parse::<i32>().ok()?;
        let _algo = parts.next()?;
        let byte_len = parts.next()?.trim().parse::<usize>().ok()?;
        if byte_len == 0 {
            continue;
        }
        let payload_end = pos.checked_add(byte_len)?;
        let payload = data.get(pos..payload_end)?;
        pos = payload_end;

        let flag = *payload.first()?;
        let body = payload.get(1..)?;
        let planar: std::borrow::Cow<'_, [u8]> = if flag == 0 {
            if body.len() != expected_tile_bytes {
                continue;
            }
            std::borrow::Cow::Borrowed(body)
        } else if flag == 1 {
            match lzf_decompress(body, expected_tile_bytes) {
                Some(dec) => std::borrow::Cow::Owned(dec),
                None => continue,
            }
        } else {
            continue;
        };

        for ly in 0..tile_h {
            let cy = i64::from(top) + i64::from(layer_y) + ly as i64;
            if cy < 0 || cy >= h as i64 {
                continue;
            }
            let cy_u = cy as usize;
            let row_base = ly * tile_w;
            for lx in 0..tile_w {
                let cx = i64::from(left) + i64::from(layer_x) + lx as i64;
                if cx < 0 || cx >= w as i64 {
                    continue;
                }
                let cx_u = cx as usize;
                let p = row_base + lx;
                let (r, g, b, a) = match pixel_size {
                    4 => {
                        let b = planar.get(p).copied().unwrap_or(0);
                        let g = planar.get(tile_pixels + p).copied().unwrap_or(0);
                        let r = planar.get(tile_pixels * 2 + p).copied().unwrap_or(0);
                        let a = planar.get(tile_pixels * 3 + p).copied().unwrap_or(0);
                        (r, g, b, a)
                    }
                    8 => {
                        // 16-bit integer BGRA planar: take high byte of each little-endian u16 channel
                        let b = planar.get(tile_pixels + p).copied().unwrap_or(0);
                        let g = planar.get(tile_pixels * 3 + p).copied().unwrap_or(0);
                        let r = planar.get(tile_pixels * 5 + p).copied().unwrap_or(0);
                        let a = planar.get(tile_pixels * 7 + p).copied().unwrap_or(0);
                        (r, g, b, a)
                    }
                    2 => {
                        // 8-bit grayscale + alpha
                        let y = planar.get(p).copied().unwrap_or(0);
                        let a = planar.get(tile_pixels + p).copied().unwrap_or(0);
                        (y, y, y, a)
                    }
                    1 => {
                        let y = planar.get(p).copied().unwrap_or(0);
                        (y, y, y, 255)
                    }
                    _ => (0, 0, 0, 0),
                };
                if a == 0 {
                    continue;
                }
                // Premultiply for DreamsCraft internal RGBA8 format
                let a16 = u16::from(a);
                let premul = |c: u8| ((u16::from(c) * a16 + 127) / 255) as u8;
                let dst = (cy_u * w + cx_u) * 4;
                if let Some(px) = out.get_mut(dst..dst + 4) {
                    px.copy_from_slice(&[premul(r), premul(g), premul(b), a]);
                }
            }
        }
    }

    Some(out)
}

fn make_uuid(counter: u32) -> String {
    format!("{{d7ea4c01-0000-4000-8000-{counter:012x}}}")
}

fn write_into<W: std::io::Write>(e: &Export<'_>, zip: &mut ZipWriter<W>) {
    // Entry 1 MUST be uncompressed `mimetype` per Krita / Calligra ODF ZIP specification.
    zip.add_aligned("mimetype", MIME.as_bytes(), 1);

    let doc = e.document;
    let (w, h) = (doc.width(), doc.height());
    let doc_folder = sanitize_doc_folder(e.name);
    let mut layer_counter = 0u32;
    let mut layers_xml = String::new();
    let mut animation_xml = String::new();

    if e.options.export_animation
        && let Some(tl) = doc.timeline.as_ref()
    {
        write_timeline_layers(e, zip, tl, &doc_folder, &mut layer_counter, &mut layers_xml, &mut animation_xml);
    } else if e.options.export_animation
        && let Some(anim) = doc.animation.as_ref()
        && !anim.pages
    {
        write_legacy_animation_layers(
            e,
            zip,
            anim,
            &doc_folder,
            &mut layer_counter,
            &mut layers_xml,
            &mut animation_xml,
        );
    } else {
        // Export standard Document layers & groups in Krita top-to-bottom order
        let mut tops: Vec<LayerId> = doc.members(None);
        tops.reverse();
        for id in tops {
            write_doc_layer_xml(e, zip, id, &doc_folder, 3, &mut layer_counter, &mut layers_xml);
        }
    }

    let dpi = if doc.dpi.is_finite() && doc.dpi > 0.0 { doc.dpi } else { 72.0 };
    let maindoc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE DOC PUBLIC '-//KDE//DTD krita 2.0//EN' 'http://www.calligra.org/DTD/krita-2.0.dtd'>\n\
<DOC xmlns=\"http://www.calligra.org/DTD/krita\" editor=\"DreamsCraft\" syntaxVersion=\"2\" kritaVersion=\"5.2.0\">\n\
 <IMAGE width=\"{w}\" height=\"{h}\" mime=\"application/x-kra\" name=\"{}\" description=\"\" colorspacename=\"RGBA\" profile=\"sRGB-elle-V2-srgbtrc.icc\" x-res=\"{dpi}\" y-res=\"{dpi}\">\n\
  <layers>\n\
{layers_xml}\
  </layers>\n\
{animation_xml}\
 </IMAGE>\n\
</DOC>\n",
        xml_escape(&doc_folder),
    );
    zip.add("maindoc.xml", maindoc.as_bytes());

    let docinfo = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE document-info PUBLIC '-//KDE//DTD document-info 1.1//EN' 'http://www.calligra.org/DTD/document-info-1.1.dtd'>\n\
<document-info xmlns=\"http://www.calligra.org/DTD/document-info\">\n\
 <about>\n\
  <title>{}</title>\n\
  <description/>\n\
  <abstract><![CDATA[]]></abstract>\n\
 </about>\n\
 <author>\n\
  <full-name>{}</full-name>\n\
 </author>\n\
</document-info>\n",
        xml_escape(e.name),
        xml_escape(e.author),
    );
    zip.add("documentinfo.xml", docinfo.as_bytes());

    if e.options.include_preview {
        if !e.merged_png.is_empty() {
            zip.add("mergedimage.png", e.merged_png);
        }
        if !e.preview_png.is_empty() {
            zip.add("preview.png", e.preview_png);
        }
    }
}

fn write_layer_tiles<W: std::io::Write>(
    e: &Export<'_>,
    zip: &mut ZipWriter<W>,
    entry_path: &str,
    layer_id: Option<LayerId>,
) {
    let doc = e.document;
    let px = layer_id.and_then(|id| {
        e.pixels.pixels(id).unwrap_or_else(|err| {
            zip.fail(err);
            None
        })
    });
    match px {
        Some(bytes) => {
            let encoded = encode_kra_v2_tiles(&bytes, doc.width(), doc.height());
            zip.add(entry_path, &encoded);
        }
        None => {
            zip.add(entry_path, empty_kra_v2_tiles());
        }
    }
}

fn write_doc_layer_xml<W: std::io::Write>(
    e: &Export<'_>,
    zip: &mut ZipWriter<W>,
    id: LayerId,
    doc_folder: &str,
    indent: usize,
    counter: &mut u32,
    out: &mut String,
) {
    let doc = e.document;
    let Some(layer) = doc.layer(id) else { return };
    if layer.kind == LayerKind::Mask {
        return;
    }
    let pad = " ".repeat(indent);
    let idx = *counter;
    *counter = counter.saturating_add(1);
    let fname = format!("layer{idx}");
    let uuid = make_uuid(idx);
    let opacity = (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let visible = u8::from(layer.visible);
    let locked = u8::from(layer.locked);
    let alpha_locked = u8::from(layer.alpha_lock);
    let blend = blend_to_krita(layer.blend);

    if layer.kind == LayerKind::Group {
        out.push_str(&format!(
            "{pad}<layer name=\"{}\" nodetype=\"grouplayer\" opacity=\"{opacity}\" visible=\"{visible}\" locked=\"{locked}\" compositeop=\"{blend}\" collapsed=\"{}\" filename=\"{fname}\" x=\"0\" y=\"0\" uuid=\"{uuid}\">\n",
            xml_escape(&layer.name),
            u8::from(layer.collapsed),
        ));
        out.push_str(&format!("{pad} <layers>\n"));
        let mut children = doc.members(Some(id));
        children.reverse();
        for child in children {
            write_doc_layer_xml(e, zip, child, doc_folder, indent + 2, counter, out);
        }
        out.push_str(&format!("{pad} </layers>\n{pad}</layer>\n"));
    } else {
        let entry = format!("{doc_folder}/layers/{fname}");
        write_layer_tiles(e, zip, &entry, Some(id));
        out.push_str(&format!(
            "{pad}<layer name=\"{}\" nodetype=\"paintlayer\" opacity=\"{opacity}\" visible=\"{visible}\" locked=\"{locked}\" alphaLocked=\"{alpha_locked}\" compositeop=\"{blend}\" colorspacename=\"RGBA\" filename=\"{fname}\" x=\"0\" y=\"0\" uuid=\"{uuid}\"/>\n",
            xml_escape(&layer.name),
        ));
    }
}

/// A timed cel event on one Krita animated layer (`None` layer means an explicit blank frame
/// that stops Krita from holding the previous drawing across an empty timeline gap).
#[derive(Clone, Copy)]
struct KraFrameEvent {
    time: u32,
    layer: Option<LayerId>,
}

fn collect_track_events(track: &Track, range_to: u32) -> Vec<KraFrameEvent> {
    let mut clips: Vec<&Clip> = track.clips.iter().filter(|c| !matches!(c.kind, ClipKind::Audio { .. })).collect();
    clips.sort_by_key(|c| c.start_frame);

    let mut raw: Vec<(u32, u32, Option<LayerId>)> = Vec::new();
    for clip in clips {
        match &clip.kind {
            ClipKind::Cel { cels } => {
                let mut cursor = clip.start_frame;
                for &cel in cels {
                    let end = cursor.saturating_add(cel.duration_frames());
                    raw.push((cursor, end, Some(cel.layer)));
                    cursor = end;
                }
            }
            ClipKind::Layer { layer } | ClipKind::Video { layer, .. } | ClipKind::Text { layer } => {
                raw.push((clip.start_frame, clip.end_frame(), Some(*layer)));
            }
            ClipKind::Group { members } => {
                if let Some(&first) = members.first() {
                    raw.push((clip.start_frame, clip.end_frame(), Some(first)));
                }
            }
            ClipKind::Audio { .. } => {}
        }
    }

    let mut events: Vec<KraFrameEvent> = Vec::new();
    for (i, &(start, end, layer)) in raw.iter().enumerate() {
        events.push(KraFrameEvent { time: start, layer });
        let next_start = raw.get(i + 1).map(|n| n.0);
        // Emit an explicit empty frame whenever there is a gap after this cel so Krita doesn't hold it forever
        let has_gap_after = match next_start {
            Some(ns) => ns > end,
            None => end <= range_to,
        };
        if has_gap_after {
            events.push(KraFrameEvent { time: end, layer: None });
        }
    }
    // Deduplicate identical timestamps (later event wins)
    events.sort_by_key(|ev| ev.time);
    let mut deduped: Vec<KraFrameEvent> = Vec::new();
    for ev in events {
        if let Some(last) = deduped.last_mut()
            && last.time == ev.time
        {
            *last = ev;
        } else {
            deduped.push(ev);
        }
    }
    deduped
}

fn write_timeline_layers<W: std::io::Write>(
    e: &Export<'_>,
    zip: &mut ZipWriter<W>,
    tl: &Timeline,
    doc_folder: &str,
    counter: &mut u32,
    layers_xml: &mut String,
    animation_xml: &mut String,
) {
    let range_from = tl.work_area.map_or(0, |w| w.start);
    let range_to = tl.work_area.map_or_else(|| tl.duration_frames.max(1).saturating_sub(1), |w| w.end);
    let fps = tl.fps.clamp(1, 60);
    let current = tl.playhead.min(range_to);

    animation_xml.push_str(&format!(
        "  <animation>\n\
   <framerate value=\"{fps}\" type=\"value\"/>\n\
   <range from=\"{range_from}\" to=\"{range_to}\" type=\"timerange\"/>\n\
   <currentTime value=\"{current}\" type=\"value\"/>\n\
  </animation>\n"
    ));

    // Krita lists layers TOP-TO-BOTTOM (reverse of DreamsCraft's bottom-to-top track stack)
    for track in tl.tracks.iter().rev() {
        if track.is_group {
            continue;
        }
        let events = collect_track_events(track, range_to);
        if events.is_empty() {
            continue;
        }
        let idx = *counter;
        *counter = counter.saturating_add(1);
        let fname = format!("layer{idx}");
        let uuid = make_uuid(idx);
        let opacity = (track.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let visible = u8::from(track.visible);
        let locked = u8::from(track.locked);
        let alpha_locked = u8::from(track.alpha_lock);
        let blend = blend_to_krita(track.blend);

        let is_static_full = events.len() == 1 && events.first().is_some_and(|ev| ev.time == 0 && ev.layer.is_some());
        if is_static_full {
            let layer_id = events.first().and_then(|ev| ev.layer);
            write_layer_tiles(e, zip, &format!("{doc_folder}/layers/{fname}"), layer_id);
            layers_xml.push_str(&format!(
                "   <layer name=\"{}\" nodetype=\"paintlayer\" opacity=\"{opacity}\" visible=\"{visible}\" locked=\"{locked}\" alphaLocked=\"{alpha_locked}\" compositeop=\"{blend}\" colorspacename=\"RGBA\" filename=\"{fname}\" x=\"0\" y=\"0\" uuid=\"{uuid}\"/>\n",
                xml_escape(&track.name),
            ));
        } else {
            // Write current-time frame as the base layer image, plus every frame in `layerN.fK` + `layerN.keyframes`
            let active_layer = events
                .iter()
                .rev()
                .find(|ev| ev.time <= current)
                .and_then(|ev| ev.layer)
                .or_else(|| events.iter().find_map(|ev| ev.layer));
            write_layer_tiles(e, zip, &format!("{doc_folder}/layers/{fname}"), active_layer);

            let kf_file = format!("{fname}.keyframes");
            let mut kf_xml = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE krita-keyframes>\n\
<keyframes xmlns=\"http://www.calligra.org/DTD/krita\">\n\
 <channel name=\"content\">\n",
            );
            for (fi, ev) in events.iter().enumerate() {
                let frame_name = format!("{fname}.f{fi}");
                write_layer_tiles(e, zip, &format!("{doc_folder}/layers/{frame_name}"), ev.layer);
                kf_xml.push_str(&format!(
                    "  <keyframe time=\"{}\" frame=\"{frame_name}\" color-label=\"0\"/>\n",
                    ev.time
                ));
            }
            kf_xml.push_str(" </channel>\n</keyframes>\n");
            zip.add(&format!("{doc_folder}/layers/{kf_file}"), kf_xml.as_bytes());

            layers_xml.push_str(&format!(
                "   <layer name=\"{}\" nodetype=\"paintlayer\" opacity=\"{opacity}\" visible=\"{visible}\" locked=\"{locked}\" alphaLocked=\"{alpha_locked}\" compositeop=\"{blend}\" colorspacename=\"RGBA\" filename=\"{fname}\" keyframes=\"{kf_file}\" x=\"0\" y=\"0\" uuid=\"{uuid}\"/>\n",
                xml_escape(&track.name),
            ));
        }
    }
}

fn write_legacy_animation_layers<W: std::io::Write>(
    e: &Export<'_>,
    zip: &mut ZipWriter<W>,
    anim: &crate::animation::Animation,
    doc_folder: &str,
    counter: &mut u32,
    layers_xml: &mut String,
    animation_xml: &mut String,
) {
    let doc = e.document;
    let paint_layers: Vec<LayerId> =
        doc.layers().iter().filter(|l| l.kind == LayerKind::Paint && l.visible).map(|l| l.id).collect();
    if paint_layers.is_empty() {
        return;
    }
    let idx = *counter;
    *counter = counter.saturating_add(1);
    let fname = format!("layer{idx}");
    let kf_file = format!("{fname}.keyframes");
    let uuid = make_uuid(idx);

    write_layer_tiles(e, zip, &format!("{doc_folder}/layers/{fname}"), paint_layers.first().copied());
    let mut kf_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE krita-keyframes>\n\
<keyframes xmlns=\"http://www.calligra.org/DTD/krita\">\n\
 <channel name=\"content\">\n",
    );
    let mut cursor = 0u32;
    for (fi, &lid) in paint_layers.iter().enumerate() {
        let frame_name = format!("{fname}.f{fi}");
        write_layer_tiles(e, zip, &format!("{doc_folder}/layers/{frame_name}"), Some(lid));
        kf_xml.push_str(&format!("  <keyframe time=\"{cursor}\" frame=\"{frame_name}\" color-label=\"0\"/>\n"));
        cursor = cursor.saturating_add(1 + anim.hold(lid));
    }
    kf_xml.push_str(" </channel>\n</keyframes>\n");
    zip.add(&format!("{doc_folder}/layers/{kf_file}"), kf_xml.as_bytes());

    let range_to = cursor.max(1).saturating_sub(1);
    animation_xml.push_str(&format!(
        "  <animation>\n\
   <framerate value=\"{}\" type=\"value\"/>\n\
   <range from=\"0\" to=\"{range_to}\" type=\"timerange\"/>\n\
   <currentTime value=\"0\" type=\"value\"/>\n\
  </animation>\n",
        anim.fps.clamp(1, 60)
    ));
    layers_xml.push_str(&format!(
        "   <layer name=\"Animation\" nodetype=\"paintlayer\" opacity=\"255\" visible=\"1\" locked=\"0\" compositeop=\"normal\" colorspacename=\"RGBA\" filename=\"{fname}\" keyframes=\"{kf_file}\" x=\"0\" y=\"0\" uuid=\"{uuid}\"/>\n"
    ));
}

// ---- Minimal Clean-Room XML Reader for Krita `maindoc.xml`, `documentinfo.xml`, and `*.keyframes` ----

#[derive(Clone, Debug)]
struct XmlNode {
    tag: String,
    attrs: HashMap<String, String>,
    children: Vec<XmlNode>,
    text: String,
}

impl XmlNode {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }

    fn child(&self, tag: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.tag == tag)
    }

    fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a XmlNode> {
        self.children.iter().filter(move |c| c.tag == tag)
    }
}

fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut entity = String::new();
        let mut closed = false;
        for nc in chars.by_ref() {
            if nc == ';' {
                closed = true;
                break;
            }
            entity.push(nc);
            if entity.len() > 12 {
                break;
            }
        }
        if !closed {
            out.push('&');
            out.push_str(&entity);
            continue;
        }
        match entity.as_str() {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            s if s.starts_with("#x") || s.starts_with("#X") => {
                if let Some(ch) = s.get(2..).and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            s if s.starts_with('#') => {
                if let Some(ch) = s.get(1..).and_then(|d| d.parse::<u32>().ok()).and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            other => {
                out.push('&');
                out.push_str(other);
                out.push(';');
            }
        }
    }
    out
}

fn parse_xml(src: &str) -> Option<XmlNode> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;

    while i < bytes.len() {
        if bytes.get(i).copied() == Some(b'<') {
            let rem = src.get(i..)?;
            if rem.starts_with("<!--") {
                let end = rem.find("-->")?;
                i += end + 3;
                continue;
            }
            if rem.starts_with("<![CDATA[") {
                let end = rem.find("]]>")?;
                if let Some(top) = stack.last_mut()
                    && let Some(cdata) = rem.get(9..end)
                {
                    top.text.push_str(cdata);
                }
                i += end + 3;
                continue;
            }
            if rem.starts_with("<?") {
                let end = rem.find("?>")?;
                i += end + 2;
                continue;
            }
            if rem.starts_with("<!") {
                let end = rem.find('>')?;
                i += end + 1;
                continue;
            }
            if rem.starts_with("</") {
                let end = rem.find('>')?;
                let close_tag = rem.get(2..end)?.trim();
                i += end + 1;
                if let Some(done) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(done);
                    } else if done.tag == close_tag || root.is_none() {
                        root = Some(done);
                    }
                }
                continue;
            }
            // Opening or self-closing tag
            i += 1;
            let tag_start = i;
            while i < bytes.len() {
                let b = bytes.get(i).copied().unwrap_or(b'>');
                if b.is_ascii_whitespace() || b == b'>' || b == b'/' {
                    break;
                }
                i += 1;
            }
            let tag = src.get(tag_start..i)?.to_owned();
            let mut attrs = HashMap::new();
            let mut self_closing = false;

            while i < bytes.len() {
                while i < bytes.len() && bytes.get(i).copied().is_some_and(|b| b.is_ascii_whitespace()) {
                    i += 1;
                }
                if bytes.get(i).copied() == Some(b'/') && bytes.get(i + 1).copied() == Some(b'>') {
                    self_closing = true;
                    i += 2;
                    break;
                }
                if bytes.get(i).copied() == Some(b'>') {
                    i += 1;
                    break;
                }
                let k_start = i;
                while i < bytes.len() {
                    let b = bytes.get(i).copied().unwrap_or(b'>');
                    if b == b'=' || b.is_ascii_whitespace() || b == b'>' || b == b'/' {
                        break;
                    }
                    i += 1;
                }
                let key = src.get(k_start..i)?.to_owned();
                while i < bytes.len() && bytes.get(i).copied().is_some_and(|b| b.is_ascii_whitespace()) {
                    i += 1;
                }
                if bytes.get(i).copied() != Some(b'=') {
                    continue;
                }
                i += 1;
                while i < bytes.len() && bytes.get(i).copied().is_some_and(|b| b.is_ascii_whitespace()) {
                    i += 1;
                }
                let quote = bytes.get(i).copied()?;
                if quote != b'"' && quote != b'\'' {
                    break;
                }
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes.get(i).copied() != Some(quote) {
                    i += 1;
                }
                let val = xml_unescape(src.get(v_start..i)?);
                if i < bytes.len() {
                    i += 1;
                }
                if !key.is_empty() {
                    attrs.insert(key, val);
                }
            }
            let node = XmlNode { tag, attrs, children: Vec::new(), text: String::new() };
            if self_closing {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            } else {
                stack.push(node);
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes.get(i).copied() != Some(b'<') {
                i += 1;
            }
            if let Some(top) = stack.last_mut()
                && let Some(text) = src.get(start..i)
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    top.text.push_str(&xml_unescape(trimmed));
                }
            }
        }
    }

    root.or_else(|| stack.into_iter().next())
}

fn find_layer_entry(zip: &ZipReader<'_>, doc_name: &str, filename: &str) -> Option<Vec<u8>> {
    let direct = format!("{doc_name}/layers/{filename}");
    if let Some(data) = zip.read(&direct) {
        return Some(data);
    }
    let suffix = format!("/layers/{filename}");
    let found = zip.names().find(|n| n.ends_with(&suffix) || *n == filename).map(str::to_owned)?;
    zip.read(&found)
}

fn is_blank_rgba(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>().0.iter().all(|px| px[3] == 0)
}

struct ParsedKraKeyframes {
    entries: Vec<(u32, String)>,
}

fn parse_keyframes_xml(bytes: &[u8]) -> Option<ParsedKraKeyframes> {
    let text = std::str::from_utf8(bytes).ok()?;
    let root = parse_xml(text)?;
    let mut entries: Vec<(u32, String)> = Vec::new();
    for channel in root.children_named("channel") {
        let is_content = channel.attr("name").is_none_or(|n| n == "content");
        if !is_content {
            continue;
        }
        for kf in channel.children_named("keyframe") {
            let Some(time) = kf.attr("time").and_then(|t| t.parse::<u32>().ok()) else { continue };
            let Some(frame) = kf.attr("frame") else { continue };
            entries.push((time, frame.to_owned()));
        }
    }
    entries.sort_by_key(|(t, _)| *t);
    entries.dedup_by_key(|(t, _)| *t);
    Some(ParsedKraKeyframes { entries })
}

struct ImportedAnimTrack {
    name: String,
    visible: bool,
    locked: bool,
    alpha_lock: bool,
    opacity: f32,
    blend: BlendMode,
    /// `None` => static track referencing `static_layer`; `Some(frames)` => `(time, layer, is_blank)`.
    keyframes: Option<Vec<(u32, LayerId, bool)>>,
    static_layer: Option<LayerId>,
}

/// Reads a Krita `.kra` archive into a DreamsCraft [`Document`] (with [`Timeline`] if animated)
/// and premultiplied RGBA8 layer pixel buffers.
pub fn read(bytes: &[u8]) -> Result<Imported, ReadError> {
    let zip = ZipReader::new(bytes).ok_or(ReadError::NotAKra)?;
    let maindoc_bytes = zip.read("maindoc.xml").ok_or(ReadError::NotAKra)?;
    let maindoc_str = std::str::from_utf8(&maindoc_bytes).map_err(|_| ReadError::Damaged("maindoc utf8"))?;
    let root = parse_xml(maindoc_str).ok_or(ReadError::Damaged("maindoc xml"))?;
    let image = root.child("IMAGE").ok_or(ReadError::Damaged("image"))?;

    let w = image.attr("width").and_then(|s| s.parse::<u32>().ok()).ok_or(ReadError::Damaged("width"))?;
    let h = image.attr("height").and_then(|s| s.parse::<u32>().ok()).ok_or(ReadError::Damaged("height"))?;
    if w == 0 || h == 0 {
        return Err(ReadError::Damaged("size"));
    }
    if w > MAX_CANVAS_EDGE || h > MAX_CANVAS_EDGE {
        return Err(ReadError::TooLarge);
    }
    let dpi =
        image.attr("x-res").and_then(|s| s.parse::<f32>().ok()).filter(|d| d.is_finite() && *d > 0.0).unwrap_or(72.0);
    let doc_folder = image.attr("name").unwrap_or("DreamsCraft").to_owned();

    // Parse optional animation settings from <animation>
    let anim_node = image.child("animation");
    let mut anim_fps = 24u32;
    let mut anim_from = 0u32;
    let mut anim_to = 0u32;
    let mut anim_current = 0u32;
    let mut has_animation = false;
    if let Some(anim) = anim_node {
        has_animation = true;
        if let Some(fps) = anim.child("framerate").and_then(|n| n.attr("value")).and_then(|v| v.parse::<u32>().ok()) {
            anim_fps = fps.clamp(1, 60);
        }
        if let Some(range) = anim.child("range") {
            anim_from = range.attr("from").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            anim_to = range.attr("to").and_then(|v| v.parse::<u32>().ok()).unwrap_or(23);
        }
        if let Some(cur) = anim.child("currentTime").and_then(|n| n.attr("value")).and_then(|v| v.parse::<u32>().ok()) {
            anim_current = cur;
        }
    }

    let mut layers: Vec<Layer> = Vec::new();
    let mut pixels: Vec<(LayerId, Vec<u8>)> = Vec::new();
    let mut next_id = 1u64;
    let mut anim_tracks: Vec<ImportedAnimTrack> = Vec::new();
    let mut any_keyframes = false;

    if let Some(layers_node) = image.child("layers") {
        // Krita lists layers TOP-TO-BOTTOM; iterate in reverse to build bottom-to-top stack
        let top_to_bottom: Vec<&XmlNode> = layers_node.children_named("layer").collect();
        for node in top_to_bottom.into_iter().rev() {
            read_layer_node(
                &zip,
                &doc_folder,
                w,
                h,
                node,
                None,
                &mut layers,
                &mut pixels,
                &mut next_id,
                &mut anim_tracks,
                &mut any_keyframes,
            )?;
        }
    }

    if layers.is_empty() {
        let lid = LayerId(1);
        layers.push(Layer::plain(lid, LayerKind::Paint, "Layer 1".to_owned(), None));
    }

    let selected = layers.iter().rev().find(|l| l.kind == LayerKind::Paint).map_or(LayerId(1), |l| l.id);
    let mut document = Document::from_layers(w, h, layers, selected);
    document.dpi = dpi;

    if has_animation || any_keyframes {
        let mut max_frame = anim_to;
        for tr in &anim_tracks {
            if let Some(kfs) = &tr.keyframes {
                for &(t, _, _) in kfs {
                    max_frame = max_frame.max(t);
                }
            }
        }
        let duration = max_frame.saturating_add(1).max(1);
        let mut tl = Timeline::default();
        tl.fps = anim_fps.clamp(1, 60);
        tl.duration_frames = duration;
        tl.playhead = anim_current.min(duration.saturating_sub(1));
        if anim_from > 0 || anim_to < duration.saturating_sub(1) {
            tl.work_area = Some(crate::timeline::WorkArea { start: anim_from, end: anim_to.max(anim_from) });
        }

        let mut next_clip_id = 1u64;
        for (ti, tr) in anim_tracks.into_iter().enumerate() {
            let track_id = TrackId((ti as u64).saturating_add(1));
            let color_tag = crate::timeline::TRACK_COLORS
                .get(ti % crate::timeline::TRACK_COLORS.len())
                .copied()
                .unwrap_or(0xFF3B82F6);
            let mut clips: Vec<Clip> = Vec::new();

            if let Some(kfs) = tr.keyframes {
                // Convert Krita keyframes (including blank stop-hold frames) into DreamsCraft CelClips
                let mut current_start: Option<u32> = None;
                let mut current_cels: Vec<Cel> = Vec::new();
                for (i, &(time, lid, is_blank)) in kfs.iter().enumerate() {
                    let next_time = kfs.get(i + 1).map_or(duration, |n| n.0);
                    if is_blank {
                        if let Some(start) = current_start.take()
                            && !current_cels.is_empty()
                        {
                            let cid = ClipId(next_clip_id);
                            next_clip_id += 1;
                            let mut clip = Clip {
                                id: cid,
                                name: tr.name.clone(),
                                start_frame: start,
                                duration_frames: 1,
                                blend: BlendMode::Normal,
                                opacity: 1.0,
                                kind: ClipKind::Cel { cels: std::mem::take(&mut current_cels) },
                                keyframes: Default::default(),
                                perform_onion_guide: false,
                                perform_guide_layer: None,
                                perform_guide_opacity: 0.45,
                                perform_guide_color: [0.2, 0.78, 1.0],
                            };
                            clip.sync_duration();
                            clips.push(clip);
                        }
                        continue;
                    }
                    let span = next_time.saturating_sub(time).max(1);
                    let hold = span.saturating_sub(1);
                    if current_start.is_none() {
                        current_start = Some(time);
                    }
                    current_cels.push(Cel { layer: lid, hold });
                }
                if let Some(start) = current_start
                    && !current_cels.is_empty()
                {
                    let cid = ClipId(next_clip_id);
                    next_clip_id += 1;
                    let mut clip = Clip {
                        id: cid,
                        name: tr.name.clone(),
                        start_frame: start,
                        duration_frames: 1,
                        blend: BlendMode::Normal,
                        opacity: 1.0,
                        kind: ClipKind::Cel { cels: current_cels },
                        keyframes: Default::default(),
                        perform_onion_guide: false,
                        perform_guide_layer: None,
                        perform_guide_opacity: 0.45,
                        perform_guide_color: [0.2, 0.78, 1.0],
                    };
                    clip.sync_duration();
                    clips.push(clip);
                }
            } else if let Some(lid) = tr.static_layer {
                let cid = ClipId(next_clip_id);
                next_clip_id += 1;
                clips.push(Clip {
                    id: cid,
                    name: tr.name.clone(),
                    start_frame: 0,
                    duration_frames: duration,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    kind: ClipKind::Layer { layer: lid },
                    keyframes: Default::default(),
                    perform_onion_guide: false,
                    perform_guide_layer: None,
                    perform_guide_opacity: 0.45,
                    perform_guide_color: [0.2, 0.78, 1.0],
                });
            }

            if !clips.is_empty() {
                tl.tracks.push(Track {
                    id: track_id,
                    name: tr.name,
                    visible: tr.visible,
                    locked: tr.locked,
                    solo: false,
                    opacity: tr.opacity,
                    blend: tr.blend,
                    color_tag,
                    parent_group: None,
                    is_group: false,
                    expanded: true,
                    clip_to_below: false,
                    alpha_lock: tr.alpha_lock,
                    muted: false,
                    effects: Default::default(),
                    clips,
                    keyframes: Default::default(),
                });
            }
        }
        tl.active_track = tl.tracks.last().map(|t| t.id);
        tl.active_clip = tl.tracks.last().and_then(|t| t.clips.first()).map(|c| c.id);
        if !tl.tracks.is_empty() {
            document.timeline = Some(tl);
        }
    }

    let (title, author) = parse_docinfo(&zip).unwrap_or_else(|| (doc_folder, String::new()));
    let name = if title.trim().is_empty() { "Krita Artwork".to_owned() } else { title };
    Ok(Imported { document: document.repaired(), pixels, name, author })
}

#[allow(clippy::too_many_arguments)]
fn read_layer_node(
    zip: &ZipReader<'_>,
    doc_folder: &str,
    w: u32,
    h: u32,
    node: &XmlNode,
    parent: Option<LayerId>,
    layers: &mut Vec<Layer>,
    pixels: &mut Vec<(LayerId, Vec<u8>)>,
    next_id: &mut u64,
    anim_tracks: &mut Vec<ImportedAnimTrack>,
    any_keyframes: &mut bool,
) -> Result<(), ReadError> {
    if layers.len() >= MAX_LAYERS {
        return Err(ReadError::TooLarge);
    }
    let nodetype = node.attr("nodetype").unwrap_or("paintlayer");
    let name = node.attr("name").unwrap_or("Layer").to_owned();
    let visible = node.attr("visible").is_none_or(|v| v != "0");
    let locked = node.attr("locked").is_some_and(|v| v == "1");
    let alpha_lock = node.attr("alphaLocked").is_some_and(|v| v == "1");
    let opacity = node.attr("opacity").and_then(|s| s.parse::<f32>().ok()).map_or(1.0, |v| (v / 255.0).clamp(0.0, 1.0));
    let blend = node.attr("compositeop").map_or(BlendMode::Normal, blend_from_krita);
    let lx = node.attr("x").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
    let ly = node.attr("y").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);

    if nodetype == "grouplayer" {
        let gid = LayerId(*next_id);
        *next_id += 1;
        if let Some(children_node) = node.child("layers") {
            let children: Vec<&XmlNode> = children_node.children_named("layer").collect();
            for child in children.into_iter().rev() {
                read_layer_node(
                    zip,
                    doc_folder,
                    w,
                    h,
                    child,
                    Some(gid),
                    layers,
                    pixels,
                    next_id,
                    anim_tracks,
                    any_keyframes,
                )?;
            }
        }
        layers.push(Layer {
            id: gid,
            name,
            visible,
            opacity,
            blend,
            alpha_lock: false,
            clipped: false,
            locked,
            reference: false,
            kind: LayerKind::Group,
            parent,
            collapsed: node.attr("collapsed").is_some_and(|v| v == "1"),
            text: None,
            private: false,
            assisted: false,
            plugin: None,
        });
        return Ok(());
    }

    if nodetype != "paintlayer" {
        return Ok(());
    }

    // Check for animated keyframes file (`keyframes="layerN.keyframes"`)
    if let Some(kf_filename) = node.attr("keyframes").filter(|s| !s.is_empty())
        && let Some(kf_bytes) = find_layer_entry(zip, doc_folder, kf_filename)
        && let Some(parsed) = parse_keyframes_xml(&kf_bytes)
        && !parsed.entries.is_empty()
    {
        *any_keyframes = true;
        let mut track_frames: Vec<(u32, LayerId, bool)> = Vec::new();
        for (time, frame_file) in parsed.entries {
            let frame_rgba =
                find_layer_entry(zip, doc_folder, &frame_file).and_then(|raw| decode_kra_v2_tiles(&raw, w, h, lx, ly));
            let blank = frame_rgba.as_deref().is_none_or(is_blank_rgba);
            if blank {
                track_frames.push((time, LayerId(0), true));
                continue;
            }
            if layers.len() >= MAX_LAYERS {
                return Err(ReadError::TooLarge);
            }
            let lid = LayerId(*next_id);
            *next_id += 1;
            layers.push(Layer {
                id: lid,
                name: format!("{name} (F{})", time + 1),
                visible,
                opacity: 1.0,
                blend: BlendMode::Normal,
                alpha_lock,
                clipped: false,
                locked,
                reference: false,
                kind: LayerKind::Paint,
                parent,
                collapsed: false,
                text: None,
                private: false,
                assisted: false,
                plugin: None,
            });
            if let Some(rgba) = frame_rgba {
                pixels.push((lid, rgba));
            }
            track_frames.push((time, lid, false));
        }
        anim_tracks.push(ImportedAnimTrack {
            name,
            visible,
            locked,
            alpha_lock,
            opacity,
            blend,
            keyframes: Some(track_frames),
            static_layer: None,
        });
        return Ok(());
    }

    // Static paint layer
    let lid = LayerId(*next_id);
    *next_id += 1;
    let filename = node.attr("filename").unwrap_or("");
    if !filename.is_empty()
        && let Some(raw) = find_layer_entry(zip, doc_folder, filename)
        && let Some(rgba) = decode_kra_v2_tiles(&raw, w, h, lx, ly)
    {
        pixels.push((lid, rgba));
    }
    layers.push(Layer {
        id: lid,
        name: name.clone(),
        visible,
        opacity,
        blend,
        alpha_lock,
        clipped: false,
        locked,
        reference: false,
        kind: LayerKind::Paint,
        parent,
        collapsed: false,
        text: None,
        private: false,
        assisted: false,
        plugin: None,
    });
    anim_tracks.push(ImportedAnimTrack {
        name,
        visible,
        locked,
        alpha_lock,
        opacity,
        blend,
        keyframes: None,
        static_layer: Some(lid),
    });
    Ok(())
}

fn parse_docinfo(zip: &ZipReader<'_>) -> Option<(String, String)> {
    let bytes = zip.read("documentinfo.xml")?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let root = parse_xml(text)?;
    let title = root.child("about").and_then(|a| a.child("title")).map(|t| t.text.clone()).unwrap_or_default();
    let author = root.child("author").and_then(|a| a.child("full-name")).map(|t| t.text.clone()).unwrap_or_default();
    Some((title, author))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kra_static_layers_groups_and_blend_modes_roundtrip() {
        let mut doc = Document::new(96, 80);
        let l1 = doc.current();
        doc.rename(l1, "Sky");
        let l2 = doc.add_layer();
        doc.rename(l2, "Character");
        doc.set_blend(l2, BlendMode::Multiply);
        doc.set_opacity(l2, 0.5);
        doc.set_alpha_lock(l2, true);

        let mut map: HashMap<LayerId, Vec<u8>> = HashMap::new();
        let mut px1 = vec![0u8; 96 * 80 * 4];
        // Premultiplied red pixel at (10, 12)
        let idx1 = (12 * 96 + 10) * 4;
        px1[idx1..idx1 + 4].copy_from_slice(&[200, 40, 20, 255]);
        map.insert(l1, px1);

        let mut px2 = vec![0u8; 96 * 80 * 4];
        // Semi-transparent blue pixel at (70, 68) — crosses second 64x64 tile boundary!
        let idx2 = (68 * 96 + 70) * 4;
        px2[idx2..idx2 + 4].copy_from_slice(&[0, 64, 128, 128]);
        map.insert(l2, px2);

        let kra_bytes = write(&Export {
            document: &doc,
            pixels: &map,
            merged_png: b"PNG",
            preview_png: b"PRV",
            name: "Dreams Test",
            author: "Artist",
            options: ExportOptions::default(),
        });

        // Verify uncompressed mimetype is first ZIP entry
        let zip = ZipReader::new(&kra_bytes).unwrap();
        assert_eq!(zip.read("mimetype").as_deref(), Some(b"application/x-krita".as_slice()));
        assert_eq!(zip.names().next(), Some("mimetype"));

        let imported = read(&kra_bytes).unwrap();
        assert_eq!(imported.name, "Dreams Test");
        assert_eq!(imported.author, "Artist");
        assert_eq!(imported.document.width(), 96);
        assert_eq!(imported.document.height(), 80);
        assert_eq!(imported.document.layers().len(), 2);

        let r1 = &imported.document.layers()[0];
        let r2 = &imported.document.layers()[1];
        assert_eq!(r1.name, "Sky");
        assert_eq!(r2.name, "Character");
        assert_eq!(r2.blend, BlendMode::Multiply);
        assert!((r2.opacity - 0.5).abs() < 0.02);
        assert!(r2.alpha_lock);

        let got1 = imported.pixels.iter().find(|(id, _)| *id == r1.id).unwrap();
        assert_eq!(&got1.1[idx1..idx1 + 4], &[200, 40, 20, 255]);

        let got2 = imported.pixels.iter().find(|(id, _)| *id == r2.id).unwrap();
        assert_eq!(got2.1[idx2 + 3], 128);
        assert!((i32::from(got2.1[idx2 + 2]) - 128).abs() <= 1);
    }

    #[test]
    fn kra_timeline_animation_cels_holds_and_gaps_roundtrip() {
        let mut doc = Document::new(64, 64);
        let l_cel1 = doc.current();
        let l_cel2 = doc.add_layer();
        let l_bg = doc.add_layer();

        let tl = doc.ensure_timeline();
        tl.fps = 12;
        tl.duration_frames = 24;
        tl.tracks.clear();

        // Static background track
        tl.tracks.push(Track {
            id: TrackId(1),
            name: "BG".to_owned(),
            visible: true,
            locked: false,
            solo: false,
            opacity: 1.0,
            blend: BlendMode::Normal,
            color_tag: 0xFF3B82F6,
            parent_group: None,
            is_group: false,
            expanded: true,
            clip_to_below: false,
            alpha_lock: false,
            muted: false,
            effects: Default::default(),
            clips: vec![Clip {
                id: ClipId(1),
                name: "BG".to_owned(),
                start_frame: 0,
                duration_frames: 24,
                blend: BlendMode::Normal,
                opacity: 1.0,
                kind: ClipKind::Layer { layer: l_bg },
                keyframes: Default::default(),
                perform_onion_guide: false,
                perform_guide_layer: None,
                perform_guide_opacity: 0.45,
                perform_guide_color: [0.2, 0.78, 1.0],
            }],
            keyframes: Default::default(),
        });

        // Animated cel track: cel1 at frame 2 held 3 frames (frames 2..5), cel2 at frame 5 held 2 frames (frames 5..7), then gap 7..24
        tl.tracks.push(Track {
            id: TrackId(2),
            name: "Hero".to_owned(),
            visible: true,
            locked: false,
            solo: false,
            opacity: 0.8,
            blend: BlendMode::Screen,
            color_tag: 0xFFEC4899,
            parent_group: None,
            is_group: false,
            expanded: true,
            clip_to_below: false,
            alpha_lock: false,
            muted: false,
            effects: Default::default(),
            clips: vec![Clip {
                id: ClipId(2),
                name: "Hero".to_owned(),
                start_frame: 2,
                duration_frames: 5,
                blend: BlendMode::Normal,
                opacity: 1.0,
                kind: ClipKind::Cel { cels: vec![Cel { layer: l_cel1, hold: 2 }, Cel { layer: l_cel2, hold: 1 }] },
                keyframes: Default::default(),
                perform_onion_guide: false,
                perform_guide_layer: None,
                perform_guide_opacity: 0.45,
                perform_guide_color: [0.2, 0.78, 1.0],
            }],
            keyframes: Default::default(),
        });

        let mut map: HashMap<LayerId, Vec<u8>> = HashMap::new();
        let solid = |rgba: [u8; 4]| rgba.repeat(64 * 64);
        map.insert(l_bg, solid([255, 255, 255, 255]));
        map.insert(l_cel1, solid([255, 0, 0, 255]));
        map.insert(l_cel2, solid([0, 255, 0, 255]));

        let kra_bytes = write(&Export {
            document: &doc,
            pixels: &map,
            merged_png: &[],
            preview_png: &[],
            name: "AnimTest",
            author: "DreamsCraft",
            options: ExportOptions::default(),
        });

        // Verify Krita animation XML + keyframes file exist
        let zip = ZipReader::new(&kra_bytes).unwrap();
        let maindoc = String::from_utf8(zip.read("maindoc.xml").unwrap()).unwrap();
        assert!(maindoc.contains("<animation>"));
        assert!(maindoc.contains("framerate value=\"12\""));
        assert!(maindoc.contains("keyframes=\"layer0.keyframes\""));

        // Import back into DreamsCraft and verify full animation interop!
        let imported = read(&kra_bytes).unwrap();
        let itl = imported.document.timeline.as_ref().expect("timeline imported from .kra");
        assert_eq!(itl.fps, 12);
        assert_eq!(itl.duration_frames, 24);
        assert_eq!(itl.tracks.len(), 2);
        assert_eq!(itl.tracks[0].name, "BG");
        assert_eq!(itl.tracks[1].name, "Hero");
        assert_eq!(itl.tracks[1].blend, BlendMode::Screen);
        let hero_clip = &itl.tracks[1].clips[0];
        assert_eq!(hero_clip.start_frame, 2);
        match &hero_clip.kind {
            ClipKind::Cel { cels } => {
                assert_eq!(cels.len(), 2);
                assert_eq!(cels[0].hold, 2); // frames 2, 3, 4
                assert_eq!(cels[1].hold, 1); // frames 5, 6 (stopped cleanly by empty frame at 7!)
                let p0 = imported.pixels.iter().find(|(id, _)| *id == cels[0].layer).unwrap();
                assert_eq!(&p0.1[0..4], &[255, 0, 0, 255]);
                let p1 = imported.pixels.iter().find(|(id, _)| *id == cels[1].layer).unwrap();
                assert_eq!(&p1.1[0..4], &[0, 255, 0, 255]);
            }
            other => panic!("expected Cel clip, got {other:?}"),
        }
    }

    #[test]
    fn lzf_decompressor_decodes_literals_and_backreferences() {
        // Literal "ABCD" (ctrl=3 -> 4 bytes) + back-ref len=4 (ctrl=(2<<5)|0, next=3 -> offset 4, len 4) -> "ABCDABCD"
        let compressed = [3u8, b'A', b'B', b'C', b'D', (2u8 << 5), 3u8];
        let decoded = lzf_decompress(&compressed, 8).unwrap();
        assert_eq!(decoded, b"ABCDABCD");
    }
}
