use unicode_linebreak::BreakOpportunity;

use crate::fonts::data::FontData;

// ─────────────────────────────────────────────────────────────────────────────
// Tipos públicos
// ─────────────────────────────────────────────────────────────────────────────

/// Um glifo posicionado, resultado do shaping de texto.
///
/// Os valores de avanço e offset estão em unidades de fonte (não pontos PDF).
/// Para converter para pontos: `value * font_size / units_per_em`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapedGlyph {
    /// ID do glifo na fonte (índice na tabela glyf/CFF).
    pub glyph_id: u16,
    /// Avanço horizontal em unidades de fonte.
    pub x_advance: i32,
    /// Offset horizontal em unidades de fonte.
    pub x_offset: i32,
    /// Offset vertical em unidades de fonte.
    pub y_offset: i32,
    /// Índice do byte no texto original ao qual este glifo pertence (UTF-8).
    pub cluster: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Shaping
// ─────────────────────────────────────────────────────────────────────────────

/// Converte uma string em sequência de glyphs posicionados usando rustybuzz.
///
/// Aplica todas as features OpenType ativas da fonte (kerning, ligatures, etc.)
/// e retorna os glyphs com posições ajustadas.
///
/// Texto vazio retorna vetor vazio.
pub fn shape_text(font_data: &FontData, text: &str) -> Vec<ShapedGlyph> {
    if text.is_empty() {
        return vec![];
    }

    let face = match rustybuzz::Face::from_slice(&font_data.raw_bytes, 0) {
        Some(f) => f,
        None => return vec![],
    };

    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);

    let output = rustybuzz::shape(&face, &[], buffer);
    let infos = output.glyph_infos();
    let positions = output.glyph_positions();

    infos
        .iter()
        .zip(positions.iter())
        .map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id as u16,
            x_advance: pos.x_advance,
            x_offset: pos.x_offset,
            y_offset: pos.y_offset,
            cluster: info.cluster,
        })
        .collect()
}

/// Largura total de um texto shaped em pontos PDF.
pub fn shaped_text_width(glyphs: &[ShapedGlyph], font_size: f64, units_per_em: u16) -> f64 {
    let sum: i32 = glyphs.iter().map(|g| g.x_advance).sum();
    sum as f64 * font_size / units_per_em as f64
}

// ─────────────────────────────────────────────────────────────────────────────
// Line breaking
// ─────────────────────────────────────────────────────────────────────────────

/// Alinhamento horizontal do texto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    /// Justificado: distribui espaço extra entre as palavras (exceto na última linha).
    Justified,
}

/// Variante de estilo da fonte (regular, negrito, itálico ou negrito-itálico).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontVariant {
    #[default]
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

/// Uma linha de texto posicionada, resultado de `layout_paragraph`.
#[derive(Debug, Clone)]
pub struct TextLine {
    /// Glyphs posicionados que compõem esta linha.
    pub glyphs: Vec<ShapedGlyph>,
    /// Largura total da linha em pontos PDF.
    pub width: f64,
    /// Distância do topo da linha box até o baseline, em pontos PDF.
    pub baseline_offset: f64,
    /// Offset X inicial para alinhamento (0 para Left, calculado para Center/Right).
    pub x_offset: f64,
    /// Tamanho da fonte usado nesta linha, em pontos PDF.
    pub font_size: f64,
    /// Variante de fonte para esta linha (regular, bold, italic, bold-italic).
    pub variant: FontVariant,
    /// Cor de preenchimento (string CSS: hex, oklch, etc.). None = preto padrão.
    pub color: Option<String>,
}

/// Layout completo de um parágrafo: lista de linhas e altura total.
#[derive(Debug, Clone)]
pub struct ParagraphLayout {
    pub lines: Vec<TextLine>,
    /// Altura total ocupada pelo parágrafo em pontos PDF.
    pub total_height: f64,
}

/// Divide `text` em linhas respeitando a largura máxima e regras Unicode de line breaking.
///
/// Algoritmo greedy:
/// 1. Coleta break opportunities via `unicode_linebreak`.
/// 2. Acumula segmentos até exceder `max_width` — quebra na última oportunidade.
/// 3. Quebras obrigatórias (como `\n`) forçam nova linha imediatamente.
/// 4. Cada linha é shaped completa com rustybuzz (kerning inter-palavras correto).
/// 5. Alinhamento aplicado por linha.
pub fn layout_paragraph(
    text: &str,
    font: &FontData,
    font_size: f64,
    max_width: f64,
    line_height: f64,
    align: TextAlign,
) -> ParagraphLayout {
    if text.is_empty() {
        return ParagraphLayout { lines: vec![], total_height: 0.0 };
    }

    let baseline_offset = font.ascender as f64 / font.units_per_em as f64 * font_size;
    let line_height_pt = font_size * line_height;

    // ── Fase 1: determinar break points por byte range ──────────────────────
    // Cada segmento é text[seg_start..seg_end]. Acumulamos segmentos na linha
    // corrente. Ao exceder max_width, finalizamos a linha atual e iniciamos nova.
    let breaks: Vec<(usize, BreakOpportunity)> =
        unicode_linebreak::linebreaks(text).collect();

    let mut line_ranges: Vec<(usize, usize)> = Vec::new();
    let mut line_start: usize = 0;
    let mut line_width: f64 = 0.0;
    let mut seg_start: usize = 0;

    for (seg_end, opp) in &breaks {
        let seg = &text[seg_start..*seg_end];
        let seg_w = font.text_width(seg, font_size);

        match opp {
            BreakOpportunity::Mandatory => {
                // Verificar se o segmento ainda cabe na linha atual.
                if line_width + seg_w > max_width && line_width > 0.0 {
                    // Finalizar a linha sem este segmento.
                    line_ranges.push((line_start, seg_start));
                    line_start = seg_start;
                }
                // Adicionar segmento e forçar quebra.
                line_ranges.push((line_start, *seg_end));
                line_start = *seg_end;
                line_width = 0.0;
            }
            BreakOpportunity::Allowed => {
                if line_width + seg_w > max_width && line_width > 0.0 {
                    // Quebrar antes deste segmento.
                    line_ranges.push((line_start, seg_start));
                    line_start = seg_start;
                    line_width = seg_w;
                } else {
                    // Cabe: acumular.
                    line_width += seg_w;
                }
            }
        }
        seg_start = *seg_end;
    }

    // Texto remanescente (geralmente vazio, mas como fallback de segurança).
    if line_start < text.len() {
        line_ranges.push((line_start, text.len()));
    }

    // ── Fase 2: shape cada linha e aplicar alinhamento ──────────────────────
    let n_lines = line_ranges.len();
    let lines: Vec<TextLine> = line_ranges
        .into_iter()
        .enumerate()
        .map(|(i, (start, end))| {
            let line_text = &text[start..end];
            // Remover whitespace final (espaços, \n, \r) — não renderizado.
            let trimmed = line_text.trim_end_matches(|c: char| c.is_whitespace());
            let glyphs = shape_text(font, trimmed);
            let width = shaped_text_width(&glyphs, font_size, font.units_per_em);
            let is_last = i + 1 == n_lines;
            let (glyphs, x_offset) =
                apply_alignment(glyphs, width, max_width, align, is_last, font, font_size);
            TextLine { glyphs, width, baseline_offset, x_offset, font_size, variant: FontVariant::Regular, color: None }
        })
        .collect();

    let total_height = lines.len() as f64 * line_height_pt;
    ParagraphLayout { lines, total_height }
}

/// Variante de `layout_paragraph` que propaga variante de fonte e cor a todas as linhas.
pub fn layout_paragraph_styled(
    text: &str,
    font: &FontData,
    font_size: f64,
    max_width: f64,
    line_height: f64,
    align: TextAlign,
    variant: FontVariant,
    color: Option<String>,
) -> ParagraphLayout {
    let mut layout = layout_paragraph(text, font, font_size, max_width, line_height, align);
    for line in &mut layout.lines {
        line.variant = variant;
        line.color = color.clone();
    }
    layout
}

/// Aplica alinhamento a um conjunto de glyphs e retorna (glyphs, x_offset).
///
/// - `Left`: x_offset = 0, glyphs inalterados.
/// - `Center`/`Right`: calcula x_offset; glyphs inalterados.
/// - `Justified`: distribui espaço extra nos glyphs de espaço; x_offset = 0.
///   Na última linha, comporta-se como `Left`.
fn apply_alignment(
    mut glyphs: Vec<ShapedGlyph>,
    line_width: f64,
    max_width: f64,
    align: TextAlign,
    is_last_line: bool,
    font: &FontData,
    font_size: f64,
) -> (Vec<ShapedGlyph>, f64) {
    match align {
        TextAlign::Left => (glyphs, 0.0),
        TextAlign::Center => {
            let x_offset = ((max_width - line_width) / 2.0).max(0.0);
            (glyphs, x_offset)
        }
        TextAlign::Right => {
            let x_offset = (max_width - line_width).max(0.0);
            (glyphs, x_offset)
        }
        TextAlign::Justified => {
            if is_last_line || line_width >= max_width {
                return (glyphs, 0.0);
            }
            // Identificar glyphs de espaço (U+0020) pelo glyph ID na fonte.
            let space_gid = font.glyph_id(' ').map(|g| g.0).unwrap_or(u16::MAX);
            let space_count = glyphs.iter().filter(|g| g.glyph_id == space_gid).count();
            if space_count == 0 {
                return (glyphs, 0.0);
            }
            let extra_pt = max_width - line_width;
            let extra_per_space_pt = extra_pt / space_count as f64;
            // Converter de pontos PDF para unidades de fonte.
            let extra_units =
                (extra_per_space_pt * font.units_per_em as f64 / font_size).round() as i32;
            for g in &mut glyphs {
                if g.glyph_id == space_gid {
                    g.x_advance += extra_units;
                }
            }
            (glyphs, 0.0)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Testes
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::data::FontData;

    const DEJAVU_SANS: &[u8] = include_bytes!("../../fonts/DejaVuSans.ttf");

    fn font() -> FontData {
        FontData::from_bytes(DEJAVU_SANS).expect("DejaVu Sans deve parsear")
    }

    #[test]
    fn hello_produces_five_glyphs() {
        let glyphs = shape_text(&font(), "Hello");
        assert_eq!(glyphs.len(), 5, "\"Hello\" deve produzir 5 glyphs");
    }

    #[test]
    fn all_glyphs_have_positive_x_advance() {
        let glyphs = shape_text(&font(), "Hello");
        for g in &glyphs {
            assert!(g.x_advance > 0, "glyph {} deve ter x_advance > 0", g.glyph_id);
        }
    }

    #[test]
    fn empty_string_produces_no_glyphs() {
        let glyphs = shape_text(&font(), "");
        assert!(glyphs.is_empty(), "string vazia deve produzir 0 glyphs");
    }

    #[test]
    fn total_width_is_positive() {
        let glyphs = shape_text(&font(), "Hello");
        let font = font();
        let w = shaped_text_width(&glyphs, 12.0, font.units_per_em);
        assert!(w > 0.0, "largura total deve ser positiva");
    }

    #[test]
    fn shaped_width_vs_simple_advance() {
        // Comparar shaping completo com soma de avanços individuais da TASK-005.
        // Para DejaVu Sans com kerning ativo, podem diferir em pares como "AV".
        let font = font();
        let glyphs = shape_text(&font, "AV");
        let shaped_w = shaped_text_width(&glyphs, 12.0, font.units_per_em);
        let simple_w = font.text_width("AV", 12.0);
        // Ambos devem ser positivos; o shaped considera kerning
        assert!(shaped_w > 0.0, "largura shaped deve ser positiva");
        assert!(simple_w > 0.0, "largura simples deve ser positiva");
    }

    // ── Testes de layout_paragraph ───────────────────────────────────────────

    #[test]
    fn short_text_fits_single_line() {
        let font = font();
        let layout = layout_paragraph("Hello", &font, 12.0, 500.0, 1.4, TextAlign::Left);
        assert_eq!(layout.lines.len(), 1, "texto curto deve caber em 1 linha");
        assert!(layout.total_height > 0.0, "altura total deve ser positiva");
    }

    #[test]
    fn empty_text_produces_no_lines() {
        let font = font();
        let layout = layout_paragraph("", &font, 12.0, 500.0, 1.4, TextAlign::Left);
        assert_eq!(layout.lines.len(), 0, "texto vazio deve produzir 0 linhas");
        assert_eq!(layout.total_height, 0.0);
    }

    #[test]
    fn long_text_breaks_into_multiple_lines() {
        let font = font();
        let text = "Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua";
        // Largura pequena forçará quebras.
        let layout = layout_paragraph(text, &font, 12.0, 100.0, 1.4, TextAlign::Left);
        assert!(
            layout.lines.len() > 1,
            "texto longo com max_width=100 deve gerar múltiplas linhas"
        );
    }

    #[test]
    fn no_line_exceeds_max_width() {
        let font = font();
        let text = "Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor";
        let max_width = 120.0;
        let layout = layout_paragraph(text, &font, 12.0, max_width, 1.4, TextAlign::Left);
        for (i, line) in layout.lines.iter().enumerate() {
            assert!(
                line.width <= max_width + 0.1, // tolerância de arredondamento
                "linha {i} tem largura {:.2} que excede max_width {max_width}",
                line.width
            );
        }
    }

    #[test]
    fn mandatory_break_on_newline() {
        let font = font();
        let layout = layout_paragraph("Linha A\nLinha B", &font, 12.0, 500.0, 1.4, TextAlign::Left);
        assert_eq!(layout.lines.len(), 2, "\\n deve gerar 2 linhas");
    }

    #[test]
    fn very_long_word_does_not_panic() {
        let font = font();
        // Palavra sem break opportunity que excede max_width.
        let layout =
            layout_paragraph("Supercalifragilisticexpialidocious", &font, 12.0, 10.0, 1.4, TextAlign::Left);
        assert_eq!(layout.lines.len(), 1, "palavra longa deve ficar em 1 linha (overflow sem crash)");
    }

    #[test]
    fn center_alignment_produces_positive_x_offset() {
        let font = font();
        let layout = layout_paragraph("Hi", &font, 12.0, 500.0, 1.4, TextAlign::Center);
        let line = &layout.lines[0];
        assert!(line.x_offset > 0.0, "alinhamento central deve ter x_offset > 0");
    }

    #[test]
    fn right_alignment_x_offset_near_max_width() {
        let font = font();
        let layout = layout_paragraph("Hi", &font, 12.0, 500.0, 1.4, TextAlign::Right);
        let line = &layout.lines[0];
        // x_offset + line.width deve ≈ max_width
        let sum = line.x_offset + line.width;
        assert!(
            (sum - 500.0).abs() < 1.0,
            "Right: x_offset + width deve ser ≈ max_width, got {sum:.2}"
        );
    }

    #[test]
    fn justified_last_line_is_not_stretched() {
        let font = font();
        let text = "Um dois tres quatro cinco seis sete oito nove dez onze doze treze";
        let layout = layout_paragraph(text, &font, 12.0, 120.0, 1.4, TextAlign::Justified);
        // Última linha não deve ser esticada (x_offset == 0, glyphs normais).
        if let Some(last) = layout.lines.last() {
            assert_eq!(last.x_offset, 0.0, "última linha justificada deve ter x_offset = 0");
        }
    }

    #[test]
    fn total_height_equals_lines_times_line_height() {
        let font = font();
        let font_size = 12.0;
        let line_height = 1.4;
        let text = "Linha A\nLinha B\nLinha C";
        let layout = layout_paragraph(text, &font, font_size, 500.0, line_height, TextAlign::Left);
        let expected = layout.lines.len() as f64 * font_size * line_height;
        assert!(
            (layout.total_height - expected).abs() < 0.001,
            "total_height={:.3} deve ser {:.3}",
            layout.total_height,
            expected
        );
    }
}
