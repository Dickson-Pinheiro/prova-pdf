//! Fiducial corner marks (◎ concentric targets) for OMR alignment.
//!
//! The reference embeds a 30×30px PNG at each corner of the OMR area; here
//! the target is drawn vectorially (two stroked rings + a filled center dot),
//! which scans identically and keeps the sheet free of raster assets.

use crate::layout::fragment::{FilledCircle, Fragment, FragmentKind, StrokedCircle};

use super::BLACK;

/// Bounding box side of each mark (30px CSS × 0.52).
pub const MARK_SIZE: f64 = 15.6;

/// Top-left corners of the four marks, measured from the reference:
/// two flanking the orientations/registration panels, two at the page bottom.
pub const MARK_POSITIONS: [(f64, f64); 4] = [
    (25.60, 119.53),
    (554.81, 119.53),
    (26.12, 792.73),
    (554.29, 792.73),
];

// Ring geometry (fractions of MARK_SIZE, eyeballed from the reference PNG).
const OUTER_D: f64 = 13.80;
const OUTER_W: f64 = 1.80;
const MID_D: f64 = 7.80;
const MID_W: f64 = 1.60;
const DOT_D: f64 = 3.40;

pub(crate) fn push_fiducials(out: &mut Vec<Fragment>) {
    for (x, y) in MARK_POSITIONS {
        push_target(x + MARK_SIZE / 2.0, y + MARK_SIZE / 2.0, out);
    }
}

/// One concentric target centered at (`cx`, `cy`).
fn push_target(cx: f64, cy: f64, out: &mut Vec<Fragment>) {
    for (d, w) in [(OUTER_D, OUTER_W), (MID_D, MID_W)] {
        out.push(Fragment {
            x: cx - d / 2.0,
            y: cy - d / 2.0,
            width: d,
            height: d,
            kind: FragmentKind::StrokedCircle(StrokedCircle {
                stroke_width: w,
                color: BLACK.to_owned(),
            }),
        });
    }
    out.push(Fragment {
        x: cx - DOT_D / 2.0,
        y: cy - DOT_D / 2.0,
        width: DOT_D,
        height: DOT_D,
        kind: FragmentKind::FilledCircle(FilledCircle { color: BLACK.to_owned() }),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_marks_three_fragments_each() {
        let mut out = Vec::new();
        push_fiducials(&mut out);
        assert_eq!(out.len(), 12);
    }

    #[test]
    fn marks_stay_inside_their_boxes() {
        let mut out = Vec::new();
        push_fiducials(&mut out);
        for chunk in out.chunks(3) {
            let (mx, my) = {
                // Recover the box from the outer ring center.
                let f = &chunk[0];
                (f.x + f.width / 2.0 - MARK_SIZE / 2.0, f.y + f.height / 2.0 - MARK_SIZE / 2.0)
            };
            for f in chunk {
                assert!(f.x >= mx - 0.01 && f.x + f.width <= mx + MARK_SIZE + 0.01);
                assert!(f.y >= my - 0.01 && f.y + f.height <= my + MARK_SIZE + 0.01);
            }
        }
    }
}
