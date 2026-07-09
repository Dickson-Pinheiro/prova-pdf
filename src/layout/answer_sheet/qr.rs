//! QR code rendering: encodes the payload with `qrcodegen` and emits one
//! black `FilledRect` per dark module — the same representation used by the
//! Chromium reference (435 vector rects of ~1.77pt for a version-3 code).

use crate::layout::fragment::Fragment;

use super::{filled_rect, BLACK};

/// Total rendered size of the QR, independent of version (the reference
/// renders a fixed ~100px CSS box: 29 modules × 1.77pt = 51.35pt).
pub const QR_SIZE_PT: f64 = 51.35;

/// Push QR module rects centered at (`cx`, `cy`).
///
/// Encoding uses ECC level Medium with automatic version selection (the
/// reference sheet carries a version-3, 29×29 code).  No quiet zone is
/// drawn — the surrounding header cell provides the white margin, exactly
/// like the reference.  Encoding failures (payload too large) are silently
/// skipped: an unreadable sheet is worse than a missing QR, and validation
/// upstream reports the error before layout runs.
pub(crate) fn push_qr(payload: &str, cx: f64, cy: f64, out: &mut Vec<Fragment>) {
    let Ok(code) = qrcodegen::QrCode::encode_text(payload, qrcodegen::QrCodeEcc::Medium) else {
        return;
    };
    let n = code.size();
    let module = QR_SIZE_PT / n as f64;
    let x0 = cx - QR_SIZE_PT / 2.0;
    let y0 = cy - QR_SIZE_PT / 2.0;

    for my in 0..n {
        for mx in 0..n {
            if code.get_module(mx, my) {
                out.push(filled_rect(
                    x0 + mx as f64 * module,
                    y0 + my as f64 * module,
                    module,
                    module,
                    BLACK,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_produces_modules_within_bounds() {
        let mut out = Vec::new();
        push_qr("#A:1:2ea687c7-8ff8-4821-8d55-1443fe392a9c#", 534.53, 67.55, &mut out);
        assert!(out.len() > 100, "QR must produce many module rects, got {}", out.len());
        let x0 = 534.53 - QR_SIZE_PT / 2.0;
        let y0 = 67.55 - QR_SIZE_PT / 2.0;
        for f in &out {
            assert!(f.x >= x0 - 0.01 && f.x + f.width <= x0 + QR_SIZE_PT + 0.01);
            assert!(f.y >= y0 - 0.01 && f.y + f.height <= y0 + QR_SIZE_PT + 0.01);
        }
    }

    #[test]
    fn qr_42_char_payload_is_version_3() {
        // The reference tracking payload produces a 29×29 (version 3) code.
        let code = qrcodegen::QrCode::encode_text(
            "#A:1:2ea687c7-8ff8-4821-8d55-1443fe392a9c#",
            qrcodegen::QrCodeEcc::Medium,
        ).unwrap();
        assert_eq!(code.size(), 29);
    }

    #[test]
    fn qr_empty_payload_still_encodes() {
        let mut out = Vec::new();
        push_qr("", 100.0, 100.0, &mut out);
        assert!(!out.is_empty(), "empty string is a valid QR payload");
    }
}
