use ttf_parser::{Face, FaceParsingError, GlyphId};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Erros
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum FontError {
    #[error("Falha ao parsear fonte: {0:?}")]
    ParseError(FaceParsingError),

    #[error("Fonte não encontrada: {0}")]
    NotFound(String),

    #[error("Falha ao subsetar fonte: {0}")]
    SubsetError(String),
}

impl From<FaceParsingError> for FontError {
    fn from(e: FaceParsingError) -> Self {
        FontError::ParseError(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FontData — struct interna (não exposta ao JS)
// ─────────────────────────────────────────────────────────────────────────────

/// Dados e métricas de uma fonte TTF/OTF.
/// Mantém os bytes brutos para permitir re-parse ao calcular métricas de glifos.
pub struct FontData {
    pub raw_bytes: Vec<u8>,
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
}

impl FontData {
    /// Parseia bytes TTF/OTF e extrai métricas globais da fonte.
    pub fn from_bytes(data: &[u8]) -> Result<Self, FontError> {
        let face = Face::parse(data, 0)?;
        Ok(FontData {
            raw_bytes: data.to_vec(),
            units_per_em: face.units_per_em(),
            ascender: face.ascender(),
            descender: face.descender(),
            line_gap: face.line_gap(),
        })
    }

    /// Creates an empty placeholder FontData (for use as a default before registration).
    pub fn empty() -> Self {
        FontData {
            raw_bytes: Vec::new(),
            units_per_em: 1000,
            ascender: 800,
            descender: -200,
            line_gap: 0,
        }
    }

    /// Returns true if this FontData holds no actual font bytes.
    pub fn is_empty(&self) -> bool {
        self.raw_bytes.is_empty()
    }

    /// Retorna o GlyphId de um caractere, ou None se não existir na fonte.
    pub fn glyph_id(&self, c: char) -> Option<GlyphId> {
        Face::parse(&self.raw_bytes, 0).ok()?.glyph_index(c)
    }

    /// Avanço horizontal de um glifo em unidades da fonte.
    pub fn advance_width(&self, glyph: GlyphId) -> Option<u16> {
        Face::parse(&self.raw_bytes, 0).ok()?.glyph_hor_advance(glyph)
    }

    /// Largura de um texto em pontos PDF para um dado tamanho de fonte.
    ///
    /// Soma os avanços horizontais dos glifos correspondentes a cada caractere.
    /// Caracteres ausentes na fonte são ignorados.
    pub fn text_width(&self, text: &str, font_size: f64) -> f64 {
        let face = match Face::parse(&self.raw_bytes, 0) {
            Ok(f) => f,
            Err(_) => return 0.0,
        };
        let scale = font_size / self.units_per_em as f64;
        text.chars()
            .filter_map(|c| face.glyph_index(c))
            .filter_map(|g| face.glyph_hor_advance(g))
            .map(|adv| adv as f64 * scale)
            .sum()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FontFamily — conjunto de variantes de uma família de fontes
// ─────────────────────────────────────────────────────────────────────────────

/// Família de fontes com variantes regular, bold, italic e bold-italic.
///
/// Apenas a variante regular é obrigatória. As demais são opcionais;
/// se ausentes, o renderizador recorre ao regular como fallback.
pub struct FontFamily {
    pub regular: FontData,
    pub bold: Option<FontData>,
    pub italic: Option<FontData>,
    pub bold_italic: Option<FontData>,
}

impl FontFamily {
    /// Cria uma família com apenas a variante regular.
    pub fn new(regular: FontData) -> Self {
        FontFamily { regular, bold: None, italic: None, bold_italic: None }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Testes
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Embedda DejaVu Sans (incluída no diretório fonts/) para testes offline.
    const DEJAVU_SANS: &[u8] = include_bytes!("../../fonts/DejaVuSans.ttf");

    #[test]
    fn parse_units_per_em() {
        let font = FontData::from_bytes(DEJAVU_SANS).unwrap();
        assert_eq!(font.units_per_em, 2048, "DejaVu Sans deve ter 2048 units/em");
    }

    #[test]
    fn ascender_is_positive() {
        let font = FontData::from_bytes(DEJAVU_SANS).unwrap();
        assert!(font.ascender > 0, "ascender deve ser positivo");
    }

    #[test]
    fn descender_is_negative() {
        let font = FontData::from_bytes(DEJAVU_SANS).unwrap();
        assert!(font.descender < 0, "descender deve ser negativo");
    }

    #[test]
    fn glyph_id_ascii() {
        let font = FontData::from_bytes(DEJAVU_SANS).unwrap();
        assert!(font.glyph_id('A').is_some(), "DejaVu Sans deve ter glifo para 'A'");
        assert!(font.glyph_id('z').is_some(), "DejaVu Sans deve ter glifo para 'z'");
    }

    #[test]
    fn advance_width_is_positive() {
        let font = FontData::from_bytes(DEJAVU_SANS).unwrap();
        let gid = font.glyph_id('H').expect("DejaVu Sans deve ter glifo 'H'");
        let adv = font.advance_width(gid).expect("glifo 'H' deve ter advance");
        assert!(adv > 0, "advance de 'H' deve ser positivo");
    }

    #[test]
    fn text_width_hello_12pt() {
        let font = FontData::from_bytes(DEJAVU_SANS).unwrap();
        let w = font.text_width("Hello", 12.0);
        assert!(w > 0.0, "largura de 'Hello' em 12pt deve ser positiva");
        assert!(w < 100.0, "largura de 'Hello' em 12pt deve ser < 100pt, foi {w}");
    }

    #[test]
    fn text_width_empty_string() {
        let font = FontData::from_bytes(DEJAVU_SANS).unwrap();
        let w = font.text_width("", 12.0);
        assert_eq!(w, 0.0, "largura de string vazia deve ser 0");
    }

    #[test]
    fn invalid_bytes_return_error() {
        let result = FontData::from_bytes(&[0x00, 0x01, 0x02, 0x03]);
        assert!(result.is_err(), "bytes inválidos devem retornar Err");
    }

    #[test]
    fn empty_font_is_empty() {
        let f = FontData::empty();
        assert!(f.is_empty());
    }

    #[test]
    fn parsed_font_is_not_empty() {
        let f = FontData::from_bytes(DEJAVU_SANS).unwrap();
        assert!(!f.is_empty());
    }
}
