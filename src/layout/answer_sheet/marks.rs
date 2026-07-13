//! Fiducial corner marks for OMR alignment.
//!
//! The OMR reader expects a solid black square at each corner of the sheet;
//! here it is drawn vectorially as a filled rectangle that fills the marker
//! box exactly, so it scans identically and keeps the sheet free of raster
//! assets.

use crate::layout::fragment::Fragment;

use super::{filled_rect, BLACK};

/// Bounding box side of each mark (30px CSS × 0.52) — the solid square fills it.
pub const MARK_SIZE: f64 = 15.6;

/// Top-left corners of the four marks, measured from the reference:
/// two flanking the top of the orientations panel, two at the page bottom.
pub const MARK_POSITIONS: [(f64, f64); 4] = [
    (25.60, 119.53),
    (554.81, 119.53),
    (26.12, 792.73),
    (554.29, 792.73),
];

pub(crate) fn push_fiducials(out: &mut Vec<Fragment>) {
    for (x, y) in MARK_POSITIONS {
        out.push(filled_rect(x, y, MARK_SIZE, MARK_SIZE, BLACK));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::fragment::FragmentKind;

    #[test]
    fn four_solid_black_square_marks() {
        let mut out = Vec::new();
        push_fiducials(&mut out);
        assert_eq!(out.len(), 4);
        for (f, (x, y)) in out.iter().zip(MARK_POSITIONS) {
            assert!((f.x - x).abs() < 1e-9 && (f.y - y).abs() < 1e-9, "mark at wrong position");
            assert!(
                (f.width - MARK_SIZE).abs() < 1e-9 && (f.height - MARK_SIZE).abs() < 1e-9,
                "mark not a MARK_SIZE square",
            );
            assert!(
                matches!(&f.kind, FragmentKind::FilledRect(r) if r.color == "#000000"),
                "mark must be a solid black filled rectangle",
            );
        }
    }
}
