//! Image embedding and rendering for PDF output.
//!
//! Supports two formats detected from magic bytes:
//! - **JPEG** (`FF D8`) → embedded raw with `DCTDecode` filter.
//! - **PNG** (everything else) → decoded to raw RGB8 pixels, recompressed
//!   with `miniz_oxide` deflate and embedded with `FlateDecode`.
//!
//! PNG decoding requires the `images` Cargo feature.  When the feature is
//! absent every image embedding attempt returns an error; JPEG could still
//! be supported without it but is kept symmetric for simplicity.
//!
//! # Object layout (1 ref per image)
//!
//! | offset | object       |
//! |--------|--------------|
//! | +0     | ImageXObject |
//!
//! Images are deduplicated by their store key: each unique key is written
//! exactly once regardless of how many pages reference it.

use std::collections::HashMap;

use pdf_writer::{Chunk, Content, Filter, Name, Ref};

use crate::pipeline::PipelineError;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata for one embedded image XObject.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    /// Ref to the `ImageXObject` stream — used in page `/Resources /XObject`.
    pub xobject_ref:   Ref,
    /// Width in pixels.
    pub width:         u32,
    /// Height in pixels.
    pub height:        u32,
    /// PDF resource name: `Im0`, `Im1`, … (used in the `Do` operator).
    pub resource_name: String,
}

/// All images embedded in the document.
pub struct ImageMap {
    /// Maps image store key → embedded image info.
    /// Sorted by key for deterministic object numbering.
    pub images: Vec<(String, ImageInfo)>,
}

impl ImageMap {
    /// Returns `true` if no images were embedded.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Look up an image by its store key.
    pub fn get(&self, key: &str) -> Option<&ImageInfo> {
        self.images.iter().find_map(|(k, v)| (k == key).then_some(v))
    }
}

/// Number of PDF object refs consumed per image (the `ImageXObject` stream).
pub const REFS_PER_IMAGE: i32 = 1;

// ─────────────────────────────────────────────────────────────────────────────
// Format detection
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` if `data` starts with the JPEG magic bytes `FF D8`.
pub fn is_jpeg(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8
}

// ─────────────────────────────────────────────────────────────────────────────
// Embedding
// ─────────────────────────────────────────────────────────────────────────────

/// Embed all images into `chunk`, one `ImageXObject` per unique store key.
///
/// Keys are sorted alphabetically so the ref numbering is deterministic.
/// Returns an [`ImageMap`] with the metadata for every embedded image.
///
/// When `grayscale` is `true` every image is converted to grayscale using
/// Rec. 709 luminance before being written into the PDF.
pub fn embed_images(
    chunk:     &mut Chunk,
    images:    &HashMap<String, Vec<u8>>,
    base_ref:  i32,
    grayscale: bool,
) -> Result<ImageMap, PipelineError> {
    // Sort keys for reproducible output.
    let mut sorted: Vec<(&String, &Vec<u8>)> = images.iter().collect();
    sorted.sort_by_key(|(k, _)| k.as_str());

    let mut result = Vec::new();
    for (idx, (key, data)) in sorted.into_iter().enumerate() {
        let img_ref       = Ref::new(base_ref + idx as i32);
        let resource_name = format!("Im{idx}");

        let (w, h) = embed_one(chunk, data, img_ref, grayscale)
            .map_err(|e| PipelineError::EmissionError(
                format!("image '{}': {e}", key)))?;

        result.push((key.clone(), ImageInfo {
            xobject_ref: img_ref,
            width:        w,
            height:       h,
            resource_name,
        }));
    }

    Ok(ImageMap { images: result })
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-format embedding (feature-gated)
// ─────────────────────────────────────────────────────────────────────────────

/// Embed a single image, returning `(width, height)` on success.
///
/// With `features = ["images"]`: JPEG → DCTDecode (or gray FlateDecode),
/// PNG → FlateDecode.
/// Without `images` feature: always returns an error.
#[cfg(feature = "images")]
fn embed_one(chunk: &mut Chunk, data: &[u8], ref_id: Ref, grayscale: bool) -> Result<(u32, u32), String> {
    if is_jpeg(data) {
        embed_jpeg(chunk, data, ref_id, grayscale)
    } else {
        embed_png(chunk, data, ref_id, grayscale)
    }
}

#[cfg(not(feature = "images"))]
fn embed_one(
    _chunk:     &mut Chunk,
    _data:      &[u8],
    _ref_id:    Ref,
    _grayscale: bool,
) -> Result<(u32, u32), String> {
    Err("images feature not enabled".into())
}

/// Embed a JPEG image.
///
/// - Normal: keep raw compressed bytes (DCTDecode, DeviceRGB).
/// - Grayscale: decode to pixels, apply Rec. 709 luminance, embed with
///   FlateDecode + DeviceGray.
#[cfg(feature = "images")]
fn embed_jpeg(chunk: &mut Chunk, data: &[u8], ref_id: Ref, grayscale: bool) -> Result<(u32, u32), String> {
    if grayscale {
        // Decode → convert to luma → embed as raw gray pixels.
        let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
            .map_err(|e| format!("JPEG decode failed: {e}"))?;
        let luma = img.into_luma8();
        let (w, h) = luma.dimensions();
        let raw = luma.into_raw();
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        chunk.image_xobject(ref_id, &compressed)
            .width(w as i32)
            .height(h as i32)
            .color_space_name(Name(b"DeviceGray"))
            .bits_per_component(8)
            .filter(Filter::FlateDecode);
        Ok((w, h))
    } else {
        use std::io::Cursor;
        use image::ImageReader;
        let reader = ImageReader::with_format(Cursor::new(data), image::ImageFormat::Jpeg);
        let (w, h) = reader.into_dimensions()
            .map_err(|e| format!("JPEG dimensions failed: {e}"))?;
        // JPEG in PDF: passthrough raw bytes with DCTDecode (DeviceRGB).
        chunk.image_xobject(ref_id, data)
            .width(w as i32)
            .height(h as i32)
            .color_space_name(Name(b"DeviceRGB"))
            .bits_per_component(8)
            .filter(Filter::DctDecode);
        Ok((w, h))
    }
}

/// Decode a PNG to raw pixels and recompress with DEFLATE (FlateDecode).
///
/// When `grayscale` is `true`, converts RGB pixels to grayscale using
/// Rec. 709 luminance and embeds with `DeviceGray`.
#[cfg(feature = "images")]
fn embed_png(chunk: &mut Chunk, data: &[u8], ref_id: Ref, grayscale: bool) -> Result<(u32, u32), String> {
    let img = image::load_from_memory(data)
        .map_err(|e| format!("PNG load failed: {e}"))?;

    if grayscale {
        let luma = img.into_luma8();
        let (w, h) = luma.dimensions();
        let raw = luma.into_raw();
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        chunk.image_xobject(ref_id, &compressed)
            .width(w as i32)
            .height(h as i32)
            .color_space_name(Name(b"DeviceGray"))
            .bits_per_component(8)
            .filter(Filter::FlateDecode);
        Ok((w, h))
    } else {
        // Composite against white before converting to RGB8.
        // into_rgb8() drops the alpha channel, turning transparent pixels black.
        let img = match img {
            image::DynamicImage::ImageRgba8(rgba) => {
                let (w, h) = rgba.dimensions();
                let mut out = image::ImageBuffer::new(w, h);
                for (x, y, px) in rgba.enumerate_pixels() {
                    let a = px[3] as f32 / 255.0;
                    let blend = |c: u8| -> u8 { (c as f32 * a + 255.0 * (1.0 - a)) as u8 };
                    out.put_pixel(x, y, image::Rgb([blend(px[0]), blend(px[1]), blend(px[2])]));
                }
                image::DynamicImage::ImageRgb8(out)
            }
            other => other,
        };
        let img = img.into_rgb8();
        let (w, h) = img.dimensions();
        let raw = img.into_raw();
        // Compress raw pixels using DEFLATE with zlib wrapper.
        // Level 1 (fastest) — PDF viewers decompress at render time anyway.
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        chunk.image_xobject(ref_id, &compressed)
            .width(w as i32)
            .height(h as i32)
            .color_space_name(Name(b"DeviceRGB"))
            .bits_per_component(8)
            .filter(Filter::FlateDecode);
        Ok((w, h))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Content stream emission
// ─────────────────────────────────────────────────────────────────────────────

/// Emit a `Do` operator that renders an image at the fragment's position.
///
/// The image is scaled to fill the fragment box exactly.  Operators:
/// `q  w 0 0 h x pdf_y_bottom cm  /ImN Do  Q`
///
/// # Coordinate conversion
///
/// ```text
/// pdf_y_bottom = page_height − frag_y − frag_h
/// ```
pub fn emit_image_do(
    content: &mut Content,
    frag_x:  f64,
    frag_y:  f64,
    frag_w:  f64,
    frag_h:  f64,
    info:    &ImageInfo,
    ph:      f64,
) {
    let x = frag_x as f32;
    let y = (ph - frag_y - frag_h) as f32;
    let w = frag_w as f32;
    let h = frag_h as f32;

    content.save_state();
    // Scale and translate the unit-square image space to the fragment box.
    content.transform([w, 0.0, 0.0, h, x, y]);
    content.x_object(Name(info.resource_name.as_bytes()));
    content.restore_state();
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_jpeg ───────────────────────────────────────────────────────────────

    #[test]
    fn is_jpeg_detects_ff_d8_magic() {
        assert!(is_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0]));
    }

    #[test]
    fn is_jpeg_rejects_png_magic() {
        assert!(!is_jpeg(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn is_jpeg_rejects_empty() {
        assert!(!is_jpeg(&[]));
    }

    #[test]
    fn is_jpeg_rejects_single_byte() {
        assert!(!is_jpeg(&[0xFF]));
    }

    // ── ImageMap helpers ─────────────────────────────────────────────────────

    #[test]
    fn image_map_empty() {
        let map = ImageMap { images: vec![] };
        assert!(map.is_empty());
        assert!(map.get("any").is_none());
    }

    #[test]
    fn image_map_get_returns_correct_entry() {
        let info = ImageInfo {
            xobject_ref:   Ref::new(10),
            width:         100,
            height:        50,
            resource_name: "Im0".into(),
        };
        let map = ImageMap { images: vec![("logo".into(), info)] };
        assert!(!map.is_empty());
        let got = map.get("logo").unwrap();
        assert_eq!(got.width, 100);
        assert_eq!(got.height, 50);
        assert_eq!(got.resource_name, "Im0");
        assert!(map.get("other").is_none());
    }

    // ── embed_images (no images) ──────────────────────────────────────────────

    #[test]
    fn embed_empty_image_store_gives_empty_map() {
        let mut chunk = Chunk::new();
        let map = embed_images(&mut chunk, &HashMap::new(), 100, false).unwrap();
        assert!(map.is_empty());
        assert!(chunk.as_bytes().is_empty());
    }

    // ── emit_image_do ─────────────────────────────────────────────────────────

    #[test]
    fn emit_image_do_contains_do_operator() {
        let mut content = Content::new();
        let info = ImageInfo {
            xobject_ref:   Ref::new(10),
            width:         100,
            height:        50,
            resource_name: "Im0".into(),
        };
        emit_image_do(&mut content, 10.0, 20.0, 100.0, 50.0, &info, 841.89);
        let bytes = content.finish().into_vec();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("Do"), "must contain Do operator");
        assert!(s.contains("Im0"), "must reference Im0 resource");
    }

    #[test]
    fn emit_image_do_wrapped_in_save_restore() {
        let mut content = Content::new();
        let info = ImageInfo {
            xobject_ref:   Ref::new(10),
            width:         100,
            height:        50,
            resource_name: "Im0".into(),
        };
        emit_image_do(&mut content, 0.0, 0.0, 100.0, 50.0, &info, 841.89);
        let bytes = content.finish().into_vec();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("q\n"), "must start with q");
        assert!(s.trim_end().ends_with('Q'), "must end with Q");
    }

    #[test]
    fn emit_image_do_contains_cm_transform() {
        let mut content = Content::new();
        let info = ImageInfo {
            xobject_ref:   Ref::new(10),
            width:         200,
            height:        100,
            resource_name: "Im0".into(),
        };
        emit_image_do(&mut content, 0.0, 0.0, 200.0, 100.0, &info, 841.89);
        let bytes = content.finish().into_vec();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("cm"), "must contain cm (concat matrix) operator");
    }

    // ── format-specific (images feature) ─────────────────────────────────────

    #[cfg(feature = "images")]
    mod with_images {
        use super::*;
        use image::{DynamicImage, ImageBuffer, Rgb, ImageFormat};
        use std::io::Cursor;

        fn make_jpeg(w: u32, h: u32) -> Vec<u8> {
            let img = ImageBuffer::<Rgb<u8>, _>::new(w, h);
            let mut buf = Vec::new();
            DynamicImage::ImageRgb8(img)
                .write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)
                .unwrap();
            buf
        }

        fn make_png(w: u32, h: u32) -> Vec<u8> {
            let img = ImageBuffer::<Rgb<u8>, _>::new(w, h);
            let mut buf = Vec::new();
            DynamicImage::ImageRgb8(img)
                .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
                .unwrap();
            buf
        }

        #[test]
        fn embed_jpeg_produces_xobject_with_dct_filter() {
            let jpeg = make_jpeg(4, 4);
            assert!(is_jpeg(&jpeg), "make_jpeg should produce JPEG");
            let mut images = HashMap::new();
            images.insert("photo".into(), jpeg);
            let mut chunk = Chunk::new();
            let map = embed_images(&mut chunk, &images, 10, false).unwrap();
            assert_eq!(map.images.len(), 1);
            let info = &map.images[0].1;
            assert_eq!(info.xobject_ref, Ref::new(10));
            assert_eq!(info.resource_name, "Im0");
            // Chunk must have content (the XObject stream was written).
            assert!(!chunk.as_bytes().is_empty());
            let bytes = chunk.as_bytes();
            assert!(bytes.windows(9).any(|w| w == b"DCTDecode"), "JPEG must use DCTDecode filter");
        }

        #[test]
        fn embed_png_produces_xobject_with_flate_filter() {
            let png = make_png(4, 4);
            assert!(!is_jpeg(&png), "make_png should produce non-JPEG");
            let mut images = HashMap::new();
            images.insert("chart".into(), png);
            let mut chunk = Chunk::new();
            let map = embed_images(&mut chunk, &images, 20, false).unwrap();
            assert_eq!(map.images.len(), 1);
            let info = &map.images[0].1;
            assert_eq!(info.xobject_ref, Ref::new(20));
            let bytes = chunk.as_bytes();
            assert!(bytes.windows(11).any(|w| w == b"FlateDecode"), "PNG must use FlateDecode filter");
        }

        #[test]
        fn embed_jpeg_records_correct_dimensions() {
            let jpeg = make_jpeg(8, 4);
            let mut images = HashMap::new();
            images.insert("img".into(), jpeg);
            let mut chunk = Chunk::new();
            let map = embed_images(&mut chunk, &images, 1, false).unwrap();
            let info = &map.images[0].1;
            assert_eq!(info.width, 8);
            assert_eq!(info.height, 4);
        }

        #[test]
        fn embed_png_records_correct_dimensions() {
            let png = make_png(16, 8);
            let mut images = HashMap::new();
            images.insert("img".into(), png);
            let mut chunk = Chunk::new();
            let map = embed_images(&mut chunk, &images, 1, false).unwrap();
            let info = &map.images[0].1;
            assert_eq!(info.width, 16);
            assert_eq!(info.height, 8);
        }

        #[test]
        fn embed_two_images_assigns_sequential_refs() {
            let mut images = HashMap::new();
            images.insert("a".into(), make_jpeg(2, 2));
            images.insert("b".into(), make_png(2, 2));
            let mut chunk = Chunk::new();
            let map = embed_images(&mut chunk, &images, 50, false).unwrap();
            assert_eq!(map.images.len(), 2);
            // Keys are sorted: "a" → Im0 → ref 50, "b" → Im1 → ref 51.
            let refs: Vec<i32> = map.images.iter().map(|(_, i)| i.xobject_ref.get()).collect();
            assert!(refs.contains(&50));
            assert!(refs.contains(&51));
        }

        #[test]
        fn embed_image_page_has_xobject_in_pdf() {
            use crate::fonts::FontRegistry;
            use crate::layout::fragment::{Fragment, FragmentKind, ImageFragment};
            use crate::layout::page::PageGeometry;
            use crate::pdf::emit::PdfEmitter;
            use crate::spec::config::PrintConfig;

            let jpeg = make_jpeg(4, 4);
            let mut images = HashMap::new();
            images.insert("logo".into(), jpeg);

            let reg = FontRegistry::new();
            let emitter = PdfEmitter::new(&reg, &images, false);
            let frag = Fragment {
                x: 10.0, y: 20.0, width: 80.0, height: 40.0,
                kind: FragmentKind::Image(ImageFragment { key: "logo".into() }),
            };
            let geom = PageGeometry::from_config(&PrintConfig::default());
            let bytes = emitter.emit(vec![vec![frag]], &geom).unwrap();
            assert!(bytes.starts_with(b"%PDF-"));
            // XObject dictionary must be present.
            assert!(
                bytes.windows(7).any(|w| w == b"XObject"),
                "PDF must contain XObject resource"
            );
            // Do operator must appear in content stream.
            assert!(
                bytes.windows(2).any(|w| w == b"Do"),
                "PDF must contain Do operator"
            );
        }
    }
}
