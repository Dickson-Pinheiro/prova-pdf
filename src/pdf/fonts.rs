//! Font collection, subsetting, and embedding for PDF output.
//!
//! # Pipeline
//!
//! 1. **Collect** — scan all `GlyphRun` fragments across all pages and build
//!    the union of glyph IDs used per `(family, variant)` key.
//! 2. **Subset** — for each unique font face, call `subsetter::subset` to
//!    strip unused glyphs; the resulting bytes are much smaller than the
//!    original.
//! 3. **Embed** — write five PDF objects per font face:
//!    `Type0Font → CIDFont → FontDescriptor → FontFile2 stream → ToUnicode`.
//! 4. **Reuse** — because we collect the _union_ of all pages first, identical
//!    glyphs across pages share the same single set of objects.
//!
//! # Object layout (starting at `base_ref`, 5 refs per font)
//!
//! | offset | object            |
//! |--------|-------------------|
//! | +0     | Type0Font         |
//! | +1     | CIDFont           |
//! | +2     | FontDescriptor    |
//! | +3     | FontFile2 stream  |
//! | +4     | ToUnicode CMap    |

use std::collections::{BTreeMap, BTreeSet, HashMap};

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Chunk, Name, Rect, Ref, Str};
use subsetter::GlyphRemapper;
use ttf_parser::Face;

use crate::fonts::FontRegistry;
use crate::layout::fragment::{Fragment, FragmentKind};
use crate::pipeline::PipelineError;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Key identifying one font face: family name + variant index (0–3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontKey {
    pub family:  String,
    pub variant: u8,
}

/// PDF object metadata for one embedded font face.
#[derive(Debug, Clone)]
pub struct EmbeddedFont {
    /// Ref to the `Type0Font` object — used in page `/Resources` and `Tf`.
    pub type0_ref: Ref,
    /// PDF resource name used in `Tf` (e.g. `F0`, `F1`, …).
    pub resource_name: String,
    /// Remapper: `old_gid → new_gid` in the subset font.
    pub remapper: GlyphRemapper,
    /// `units_per_em` of the original face (needed for advance→pt conversion).
    pub units_per_em: u16,
}

/// All fonts embedded in the document.
///
/// Returned by [`embed_fonts`] and consumed by the content-stream builder
/// (TASK-028) to emit `Tf` operators and `TJ` glyph arrays.
pub struct FontMap {
    pub fonts: BTreeMap<FontKey, EmbeddedFont>,
}

impl FontMap {
    /// Look up an embedded font by family name and variant.
    pub fn get(&self, family: &str, variant: u8) -> Option<&EmbeddedFont> {
        self.fonts.get(&FontKey { family: family.to_owned(), variant })
    }

    /// True if no fonts are embedded (no `GlyphRun` fragments anywhere).
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 1 — Collect glyph sets
// ─────────────────────────────────────────────────────────────────────────────

/// Scan every `GlyphRun` in every page and collect the union of glyph IDs
/// per `FontKey`.
///
/// The result maps each unique `(family, variant)` pair to the set of all
/// glyph IDs used across the whole document.  Because we union across pages,
/// each font face is embedded only once (TASK-027 reuse requirement).
pub fn collect_glyph_sets(pages: &[Vec<Fragment>]) -> BTreeMap<FontKey, BTreeSet<u16>> {
    let mut sets: BTreeMap<FontKey, BTreeSet<u16>> = BTreeMap::new();
    for page in pages {
        for frag in page {
            if let FragmentKind::GlyphRun(run) = &frag.kind {
                let key = FontKey { family: run.font_family.to_string(), variant: run.variant };
                let entry = sets.entry(key).or_default();
                for &gid in &run.glyph_ids {
                    entry.insert(gid);
                }
            }
        }
    }
    sets
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 2 — Build GID → Unicode map (for ToUnicode CMap)
// ─────────────────────────────────────────────────────────────────────────────

/// Build a map from GID → Unicode char for the glyph IDs actually used.
///
/// Instead of scanning all 65 536 BMP codepoints, we scan targeted Unicode
/// ranges that cover the characters used in Portuguese exams:
/// - U+0000–U+024F: Basic Latin, Latin-1 Supplement, Latin Extended-A/B
/// - U+0370–U+03FF: Greek and Coptic (math symbols α, β, γ, …)
/// - U+2000–U+206F: General Punctuation (—, –, …, ', ', ", ")
/// - U+2070–U+209F: Superscripts and Subscripts
/// - U+20A0–U+20CF: Currency Symbols (€, ₹, …)
/// - U+2100–U+214F: Letterlike Symbols (ℝ, ℕ, …)
/// - U+2190–U+21FF: Arrows
/// - U+2200–U+22FF: Mathematical Operators (∑, ∫, √, ≤, ≥, …)
/// - U+2300–U+23FF: Miscellaneous Technical
/// - U+25A0–U+25FF: Geometric Shapes (■, □, ●, ○, …)
/// - U+FB00–U+FB06: Latin ligatures (fi, fl, …)
///
/// Total: ~2 100 codepoints vs 65 536 previously (~30× fewer lookups).
fn build_gid_to_unicode(face: &Face<'_>, used_gids: &BTreeSet<u16>) -> HashMap<u16, char> {
    let mut map: HashMap<u16, char> = HashMap::with_capacity(used_gids.len());
    let ranges: &[(u32, u32)] = &[
        (0x0000, 0x024F), // Basic Latin + Latin-1 + Latin Extended A/B
        (0x0370, 0x03FF), // Greek
        (0x2000, 0x206F), // General Punctuation
        (0x2070, 0x209F), // Superscripts/Subscripts
        (0x20A0, 0x20CF), // Currency
        (0x2100, 0x214F), // Letterlike
        (0x2190, 0x21FF), // Arrows
        (0x2200, 0x22FF), // Math Operators
        (0x2300, 0x23FF), // Misc Technical
        (0x25A0, 0x25FF), // Geometric Shapes
        (0xFB00, 0xFB06), // Latin ligatures
    ];
    for &(start, end) in ranges {
        for cp in start..=end {
            if let Some(c) = char::from_u32(cp) {
                if let Some(gid) = face.glyph_index(c) {
                    if used_gids.contains(&gid.0) {
                        map.entry(gid.0).or_insert(c);
                    }
                }
            }
        }
        // Early exit if we already mapped all used GIDs.
        if map.len() == used_gids.len() {
            break;
        }
    }
    map
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 3 — Subset + embed
// ─────────────────────────────────────────────────────────────────────────────

/// Number of PDF object refs consumed per embedded font face.
pub const REFS_PER_FONT: i32 = 5;

/// Subset every font in `glyph_sets` and write the PDF objects into `chunk`.
///
/// Object refs start at `base_ref` and are assigned sequentially
/// (5 refs per font face in the order they appear when iterating the
/// `BTreeMap`, which is alphabetical by `(family, variant)`).
///
/// Returns a [`FontMap`] that maps each `FontKey` to its embedded metadata.
/// Fonts whose family is not found in the registry or whose `FontData` is
/// empty are silently skipped.
pub fn embed_fonts(
    chunk:      &mut Chunk,
    registry:   &FontRegistry,
    glyph_sets: &BTreeMap<FontKey, BTreeSet<u16>>,
    base_ref:   i32,
) -> Result<FontMap, PipelineError> {
    let mut fonts = BTreeMap::new();
    let adobe_info = SystemInfo {
        registry:   Str(b"Adobe"),
        ordering:   Str(b"Identity"),
        supplement: 0,
    };

    for (idx, (key, glyph_ids)) in glyph_sets.iter().enumerate() {
        let offset         = base_ref + (idx as i32) * REFS_PER_FONT;
        let type0_ref      = Ref::new(offset);
        let cid_ref        = Ref::new(offset + 1);
        let descriptor_ref = Ref::new(offset + 2);
        let font_file_ref  = Ref::new(offset + 3);
        let cmap_ref       = Ref::new(offset + 4);

        // ── Look up font data ─────────────────────────────────────────────────
        let Some(family) = registry.get(&key.family) else { continue };
        let font_data = match key.variant {
            0 => &family.regular,
            1 => family.bold.as_ref().unwrap_or(&family.regular),
            2 => family.italic.as_ref().unwrap_or(&family.regular),
            3 => family.bold_italic.as_ref().unwrap_or(&family.regular),
            _ => &family.regular,
        };
        if font_data.is_empty() {
            continue;
        }
        let raw_bytes    = &font_data.raw_bytes;
        let units_per_em = font_data.units_per_em;

        // ── Subset ────────────────────────────────────────────────────────────
        let mut remapper = GlyphRemapper::new();
        for &gid in glyph_ids {
            remapper.remap(gid);
        }
        let subset_bytes = subsetter::subset(raw_bytes, 0, &remapper)
            .map_err(|e| PipelineError::EmissionError(
                format!("font subset failed for '{}/{}': {e}", key.family, key.variant)))?;

        // ── Extract face metrics ──────────────────────────────────────────────
        let face = Face::parse(raw_bytes, 0)
            .map_err(|e| PipelineError::EmissionError(
                format!("font parse failed for '{}/{}': {e:?}", key.family, key.variant)))?;

        let scale      = 1000.0 / units_per_em as f32;
        let ascender   = face.ascender()  as f32 * scale;
        let descender  = face.descender() as f32 * scale;
        let cap_height = face.capital_height()
            .map(|h| h as f32 * scale)
            .unwrap_or(ascender);

        let bb = face.global_bounding_box();
        let bbox_rect = Rect::new(
            bb.x_min as f32 * scale,
            bb.y_min as f32 * scale,
            bb.x_max as f32 * scale,
            bb.y_max as f32 * scale,
        );

        let italic_angle = face.italic_angle();

        // PostScript name: prefer the face's own name, fall back to key.
        let ps_name: String = {
            use ttf_parser::name_id;
            face.names()
                .into_iter()
                .find(|n| n.name_id == name_id::POST_SCRIPT_NAME)
                .and_then(|n| n.to_string())
                .unwrap_or_else(|| format!("{}-{}", key.family, key.variant))
                .replace(' ', "-")
        };

        // ── Font flags ────────────────────────────────────────────────────────
        let mut flags = FontFlags::SYMBOLIC;
        if face.is_italic() { flags |= FontFlags::ITALIC; }
        if face.is_bold()   { flags |= FontFlags::FORCE_BOLD; }

        // ── ToUnicode CMap ────────────────────────────────────────────────────
        let gid_to_unicode = build_gid_to_unicode(&face, glyph_ids);
        let mut unicode_cmap = pdf_writer::types::UnicodeCmap::new(
            Name(b"Adobe-Identity-UCS"),
            adobe_info,
        );
        for &old_gid in glyph_ids {
            if let Some(new_gid) = remapper.get(old_gid) {
                if let Some(&c) = gid_to_unicode.get(&old_gid) {
                    unicode_cmap.pair(new_gid, c);
                }
            }
        }
        let cmap_bytes = unicode_cmap.finish().into_vec();
        chunk.cmap(cmap_ref, &cmap_bytes)
            .system_info(adobe_info);

        // ── FontFile2 stream ──────────────────────────────────────────────────
        chunk.stream(font_file_ref, &subset_bytes);

        // ── FontDescriptor ────────────────────────────────────────────────────
        chunk.font_descriptor(descriptor_ref)
            .name(Name(ps_name.as_bytes()))
            .flags(flags)
            .bbox(bbox_rect)
            .italic_angle(italic_angle)
            .ascent(ascender)
            .descent(descender)
            .cap_height(cap_height)
            .stem_v(80.0)
            .font_file2(font_file_ref);

        // ── CIDFont ───────────────────────────────────────────────────────────
        // DW = 0 means the PDF cursor does not auto-advance after each glyph;
        // all advances are supplied explicitly via TJ adjustments (TASK-028).
        chunk.cid_font(cid_ref)
            .subtype(CidFontType::Type2)
            .base_font(Name(ps_name.as_bytes()))
            .system_info(adobe_info)
            .font_descriptor(descriptor_ref)
            .default_width(0.0);

        // ── Type0Font ─────────────────────────────────────────────────────────
        chunk.type0_font(type0_ref)
            .base_font(Name(ps_name.as_bytes()))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid_ref)
            .to_unicode(cmap_ref);

        // ── Record ────────────────────────────────────────────────────────────
        fonts.insert(key.clone(), EmbeddedFont {
            type0_ref,
            resource_name: format!("F{idx}"),
            remapper,
            units_per_em,
        });
    }

    Ok(FontMap { fonts })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use crate::layout::fragment::{FragmentKind, GlyphRun};
    use crate::layout::fragment::Fragment;
    use crate::test_helpers::fixtures::DEJAVU;

    fn make_registry() -> FontRegistry {
        let mut reg = FontRegistry::new();
        reg.add_variant("body", 0, DEJAVU.to_vec()).unwrap();
        reg
    }

    fn glyph_run_frag(family: &str, variant: u8, glyph_ids: Vec<u16>) -> Fragment {
        Fragment {
            x: 0.0, y: 0.0, width: 100.0, height: 12.0,
            kind: FragmentKind::GlyphRun(GlyphRun {
                glyph_ids,
                x_advances: vec![],
                x_offsets:  vec![],
                y_offsets:  vec![],
                font_size:  12.0,
                font_family: Rc::from(family),
                variant,
                color: Rc::from("#000000"),
                baseline_offset: 10.0,
            }),
        }
    }

    // ── collect_glyph_sets ────────────────────────────────────────────────────

    #[test]
    fn collect_empty_pages_gives_empty_map() {
        let sets = collect_glyph_sets(&[]);
        assert!(sets.is_empty());
    }

    #[test]
    fn collect_single_run_gives_its_glyph_ids() {
        let frag = glyph_run_frag("body", 0, vec![68, 69, 70]);
        let sets = collect_glyph_sets(&[vec![frag]]);
        let key = FontKey { family: "body".into(), variant: 0 };
        assert_eq!(sets[&key], BTreeSet::from([68, 69, 70]));
    }

    #[test]
    fn collect_unions_glyph_ids_across_pages() {
        let p1 = vec![glyph_run_frag("body", 0, vec![10, 20])];
        let p2 = vec![glyph_run_frag("body", 0, vec![20, 30])];
        let sets = collect_glyph_sets(&[p1, p2]);
        let key = FontKey { family: "body".into(), variant: 0 };
        assert_eq!(sets[&key], BTreeSet::from([10, 20, 30]));
    }

    #[test]
    fn collect_separates_different_variants() {
        let p = vec![
            glyph_run_frag("body", 0, vec![1, 2]),
            glyph_run_frag("body", 1, vec![3, 4]),
        ];
        let sets = collect_glyph_sets(&[p]);
        assert_eq!(sets.len(), 2);
        let k0 = FontKey { family: "body".into(), variant: 0 };
        let k1 = FontKey { family: "body".into(), variant: 1 };
        assert_eq!(sets[&k0], BTreeSet::from([1, 2]));
        assert_eq!(sets[&k1], BTreeSet::from([3, 4]));
    }

    #[test]
    fn collect_spacer_fragments_are_ignored() {
        use crate::layout::fragment::FragmentKind;
        let frag = Fragment { x: 0.0, y: 0.0, width: 0.0, height: 10.0, kind: FragmentKind::Spacer };
        let sets = collect_glyph_sets(&[vec![frag]]);
        assert!(sets.is_empty());
    }

    // ── build_gid_to_unicode ─────────────────────────────────────────────────

    #[test]
    fn gid_to_unicode_maps_ascii_glyphs() {
        let face = Face::parse(DEJAVU, 0).unwrap();
        let gid_a = face.glyph_index('A').unwrap().0;
        let used: BTreeSet<u16> = [gid_a].iter().copied().collect();
        let map = build_gid_to_unicode(&face, &used);
        assert_eq!(map.get(&gid_a), Some(&'A'));
    }

    #[test]
    fn gid_to_unicode_empty_gids_gives_empty_map() {
        let face = Face::parse(DEJAVU, 0).unwrap();
        let used: BTreeSet<u16> = BTreeSet::new();
        let map = build_gid_to_unicode(&face, &used);
        assert!(map.is_empty());
    }

    // ── embed_fonts ───────────────────────────────────────────────────────────

    #[test]
    fn embed_produces_valid_pdf_chunk() {
        let registry = make_registry();
        let glyph_ids: BTreeSet<u16> = [68, 69, 70].iter().copied().collect();
        let mut glyph_sets = BTreeMap::new();
        glyph_sets.insert(
            FontKey { family: "body".into(), variant: 0 },
            glyph_ids,
        );

        let mut chunk = Chunk::new();
        let font_map = embed_fonts(&mut chunk, &registry, &glyph_sets, 10).unwrap();

        // We should get one embedded font.
        assert_eq!(font_map.fonts.len(), 1);
        let key = FontKey { family: "body".into(), variant: 0 };
        let ef = &font_map.fonts[&key];
        assert_eq!(ef.type0_ref, Ref::new(10));
        assert_eq!(ef.resource_name, "F0");
        // Chunk should be non-empty (font objects were written).
        assert!(!chunk.as_bytes().is_empty());
    }

    #[test]
    fn embed_subset_is_smaller_than_original() {
        let registry = make_registry();
        let glyph_ids: BTreeSet<u16> = [68, 69, 70].iter().copied().collect(); // 'a','b','c'
        let mut glyph_sets = BTreeMap::new();
        glyph_sets.insert(FontKey { family: "body".into(), variant: 0 }, glyph_ids);

        // Compute subset size directly.
        let mut remapper = GlyphRemapper::new();
        for gid in [68u16, 69, 70] { remapper.remap(gid); }
        let subset_bytes = subsetter::subset(DEJAVU, 0, &remapper).unwrap();
        assert!(subset_bytes.len() < DEJAVU.len(),
            "subset ({} B) should be smaller than original ({} B)",
            subset_bytes.len(), DEJAVU.len());
    }

    #[test]
    fn embed_unknown_family_is_skipped() {
        let registry = make_registry();
        let glyph_ids: BTreeSet<u16> = [1, 2].iter().copied().collect();
        let mut glyph_sets = BTreeMap::new();
        glyph_sets.insert(FontKey { family: "nonexistent".into(), variant: 0 }, glyph_ids);

        let mut chunk = Chunk::new();
        let font_map = embed_fonts(&mut chunk, &registry, &glyph_sets, 10).unwrap();
        // Unknown family → skipped, no embedded fonts.
        assert!(font_map.is_empty());
    }

    #[test]
    fn embed_empty_glyph_sets_gives_empty_font_map() {
        let registry = make_registry();
        let mut chunk = Chunk::new();
        let font_map = embed_fonts(&mut chunk, &registry, &BTreeMap::new(), 10).unwrap();
        assert!(font_map.is_empty());
    }

    #[test]
    fn font_map_get_returns_correct_entry() {
        let registry = make_registry();
        let glyph_ids: BTreeSet<u16> = [68].iter().copied().collect();
        let mut glyph_sets = BTreeMap::new();
        glyph_sets.insert(FontKey { family: "body".into(), variant: 0 }, glyph_ids);

        let mut chunk = Chunk::new();
        let font_map = embed_fonts(&mut chunk, &registry, &glyph_sets, 100).unwrap();

        assert!(font_map.get("body", 0).is_some());
        assert!(font_map.get("body", 1).is_none());
        assert!(font_map.get("other", 0).is_none());
    }

    #[test]
    fn embed_two_fonts_assigns_sequential_refs() {
        let registry = make_registry();
        let glyph_ids: BTreeSet<u16> = [68].iter().copied().collect();
        let mut glyph_sets = BTreeMap::new();
        glyph_sets.insert(FontKey { family: "body".into(), variant: 0 }, glyph_ids.clone());
        glyph_sets.insert(FontKey { family: "body".into(), variant: 1 }, glyph_ids);

        let mut chunk = Chunk::new();
        // variant=1 uses bold → falls back to regular
        let font_map = embed_fonts(&mut chunk, &registry, &glyph_sets, 10).unwrap();

        // Both should be embedded.
        assert_eq!(font_map.fonts.len(), 2);
        // Keys are ordered: (body,0) < (body,1).
        let ef0 = font_map.get("body", 0).unwrap();
        let ef1 = font_map.get("body", 1).unwrap();
        // ef0 gets refs 10–14, ef1 gets refs 15–19.
        assert_eq!(ef0.type0_ref, Ref::new(10));
        assert_eq!(ef1.type0_ref, Ref::new(15));
        assert_eq!(ef0.resource_name, "F0");
        assert_eq!(ef1.resource_name, "F1");
    }
}
