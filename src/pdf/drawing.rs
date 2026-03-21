/// Primitivas de desenho PDF — geram fragmentos de operadores para content streams.
///
/// Todas as funções retornam strings de operadores PDF prontos para inserção num
/// content stream. Coordenadas em pontos PDF (Y=0 no rodapé, cresce para cima).

use crate::color::{Color, ColorResolver, PdfColor};
use crate::model::{Border, BorderStyle};

// ─────────────────────────────────────────────────────────────────────────────
// Constantes
// ─────────────────────────────────────────────────────────────────────────────

/// Cor padrão para linhas de resposta manuscritas (cinza claro).
pub const DEFAULT_ANSWER_LINE_COLOR: &str = "#CCCCCC";

/// Espaçamento padrão entre linhas de resposta (28pt ≈ uma linha manuscrita).
pub const DEFAULT_LINE_SPACING: f64 = 28.0;

// ─────────────────────────────────────────────────────────────────────────────
// Parsing de cor
// ─────────────────────────────────────────────────────────────────────────────

/// Parseia uma string de cor (qualquer formato suportado) e retorna `(r, g, b)`
/// em sRGB gamma-corrigido 0.0–1.0, pronto para emissão em operadores PDF.
///
/// Suporta: `#RRGGBB`, `#RGB`, `rgb(...)`, `rgba(...)`, `oklch(...)`.
/// Retorna `(0.0, 0.0, 0.0)` para entradas inválidas.
pub fn parse_color(s: &str) -> (f64, f64, f64) {
    Color::from_str(s)
        .map(|c| c.to_srgb())
        .unwrap_or((0.0, 0.0, 0.0))
}

/// Parseia uma string de cor e emite o operador PDF de contorno (`RG` ou `G`),
/// aplicando o modo P&B se o resolver estiver configurado.
fn resolve_stroke(s: &str, resolver: Option<&ColorResolver>) -> String {
    match resolver {
        Some(r) => r.resolve_str(s).to_stroke_ops(),
        None    => { let (r, g, b) = parse_color(s); PdfColor::Rgb(r, g, b).to_stroke_ops() }
    }
}

/// Parseia uma string de cor e emite o operador PDF de preenchimento (`rg` ou `g`),
/// aplicando o modo P&B se o resolver estiver configurado.
fn resolve_fill(s: &str, resolver: Option<&ColorResolver>) -> String {
    match resolver {
        Some(r) => r.resolve_str(s).to_fill_ops(),
        None    => { let (r, g, b) = parse_color(s); PdfColor::Rgb(r, g, b).to_fill_ops() }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Linha reta
// ─────────────────────────────────────────────────────────────────────────────

/// Gera operadores PDF para uma linha reta entre dois pontos.
///
/// Envolve os operadores em `q`/`Q` (save/restore state) para não contaminar
/// o estado gráfico do fluxo circundante.
pub fn draw_line(x1: f64, y1: f64, x2: f64, y2: f64, width: f64, color: &str) -> String {
    draw_line_resolved(x1, y1, x2, y2, width, color, None)
}

pub fn draw_line_resolved(
    x1: f64, y1: f64, x2: f64, y2: f64,
    width: f64, color: &str,
    resolver: Option<&ColorResolver>,
) -> String {
    let stroke = resolve_stroke(color, resolver);
    format!(
        "q {width:.4} w {stroke} {x1:.4} {y1:.4} m {x2:.4} {y2:.4} l S Q\n"
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Retângulo com borda
// ─────────────────────────────────────────────────────────────────────────────

/// Gera operadores PDF para um retângulo com borda (stroked).
///
/// `x`, `y` são as coordenadas do canto inferior esquerdo (coordenadas PDF).
/// `w` e `h` são a largura e altura em pontos.
pub fn draw_rect(x: f64, y: f64, w: f64, h: f64, border: &Border) -> String {
    draw_rect_resolved(x, y, w, h, border, None)
}

pub fn draw_rect_resolved(
    x: f64, y: f64, w: f64, h: f64,
    border: &Border, resolver: Option<&ColorResolver>,
) -> String {
    let line_width = border.width.unwrap_or(1.0);
    let color = border.color.as_deref().unwrap_or("#000000");
    let stroke = resolve_stroke(color, resolver);

    let dash = match border.style {
        Some(BorderStyle::Dashed) => "[6 3] 0 d\n",
        Some(BorderStyle::Dotted) => "[1 3] 0 d\n",
        _ => "[] 0 d\n",
    };

    format!("q {line_width:.4} w {stroke} {dash}{x:.4} {y:.4} {w:.4} {h:.4} re S Q\n")
}

// ─────────────────────────────────────────────────────────────────────────────
// Linhas de resposta manuscrita
// ─────────────────────────────────────────────────────────────────────────────

/// Gera `count` linhas horizontais de resposta espaçadas uniformemente.
///
/// `x`, `y` são o ponto inicial da primeira linha (topo da área de resposta).
/// As linhas são desenhadas para baixo com `spacing` pontos de distância.
/// Usa cor cinza claro para não interferir com a escrita do aluno.
pub fn draw_answer_lines(x: f64, y: f64, width: f64, count: u8, spacing: f64) -> String {
    draw_answer_lines_resolved(x, y, width, count, spacing, None)
}

pub fn draw_answer_lines_resolved(
    x: f64, y: f64, width: f64, count: u8, spacing: f64,
    resolver: Option<&ColorResolver>,
) -> String {
    let mut ops = String::new();
    for i in 0..count {
        let line_y = y - (i as f64) * spacing;
        ops.push_str(&draw_line_resolved(
            x, line_y, x + width, line_y, 0.5, DEFAULT_ANSWER_LINE_COLOR, resolver,
        ));
    }
    ops
}

// ─────────────────────────────────────────────────────────────────────────────
// Caixa de resposta
// ─────────────────────────────────────────────────────────────────────────────

/// Gera operadores PDF para uma caixa de resposta retangular.
///
/// `x`, `y` são as coordenadas do canto superior esquerdo da caixa.
/// A caixa se estende `height` pontos abaixo de `y`.
/// Usa borda padrão (0.75pt, preto, sólida) se `border` for `None`.
pub fn draw_answer_box(x: f64, y: f64, width: f64, height: f64, border: Option<&Border>) -> String {
    draw_answer_box_resolved(x, y, width, height, border, None)
}

pub fn draw_answer_box_resolved(
    x: f64, y: f64, width: f64, height: f64,
    border: Option<&Border>, resolver: Option<&ColorResolver>,
) -> String {
    let default_border = Border {
        width: Some(0.75),
        color: Some("#000000".to_string()),
        style: None,
    };
    let b = border.unwrap_or(&default_border);
    draw_rect_resolved(x, y - height, width, height, b, resolver)
}

// ─────────────────────────────────────────────────────────────────────────────
// Grid de resposta
// ─────────────────────────────────────────────────────────────────────────────

/// Gera operadores PDF para um grid de `rows` × `cols` células.
///
/// `x`, `y` são as coordenadas do canto superior esquerdo do grid.
/// A grade é desenhada com `height` pontos de altura total.
/// Retorna string vazia se `rows` ou `cols` for zero.
pub fn draw_answer_grid(x: f64, y: f64, width: f64, height: f64, rows: u8, cols: u8) -> String {
    if rows == 0 || cols == 0 {
        return String::new();
    }

    let mut ops = String::new();
    let cell_w = width / cols as f64;
    let cell_h = height / rows as f64;
    let bottom = y - height;

    // Linhas horizontais (incluindo topo e base)
    for i in 0..=rows {
        let row_y = bottom + (i as f64) * cell_h;
        ops.push_str(&draw_line(x, row_y, x + width, row_y, 0.5, "#000000"));
    }

    // Linhas verticais (incluindo esquerda e direita)
    for j in 0..=cols {
        let col_x = x + (j as f64) * cell_w;
        ops.push_str(&draw_line(col_x, bottom, col_x, bottom + height, 0.5, "#000000"));
    }

    ops
}

// ─────────────────────────────────────────────────────────────────────────────
// Retângulo preenchido (sem borda)
// ─────────────────────────────────────────────────────────────────────────────

/// Gera operadores PDF para um retângulo preenchido com cor sólida (sem borda).
///
/// `x`, `y` são as coordenadas do canto inferior esquerdo.
pub fn draw_filled_rect(x: f64, y: f64, w: f64, h: f64, color: &str) -> String {
    draw_filled_rect_resolved(x, y, w, h, color, None)
}

pub fn draw_filled_rect_resolved(
    x: f64, y: f64, w: f64, h: f64,
    color: &str, resolver: Option<&ColorResolver>,
) -> String {
    let fill = resolve_fill(color, resolver);
    format!("q {fill}{x:.4} {y:.4} {w:.4} {h:.4} re f Q\n")
}

// ─────────────────────────────────────────────────────────────────────────────
// Testes
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BorderStyle;

    // ── parse_color ───────────────────────────────────────────────────────────

    #[test]
    fn parse_color_black() {
        let (r, g, b) = parse_color("#000000");
        assert_eq!((r, g, b), (0.0, 0.0, 0.0));
    }

    #[test]
    fn parse_color_white() {
        let (r, g, b) = parse_color("#FFFFFF");
        assert!((r - 1.0).abs() < 1e-9);
        assert!((g - 1.0).abs() < 1e-9);
        assert!((b - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_color_red() {
        let (r, g, b) = parse_color("#FF0000");
        assert!((r - 1.0).abs() < 1e-9);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn parse_color_shorthand_red() {
        let (r, g, b) = parse_color("#f00");
        assert!((r - 1.0).abs() < 1e-9);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn parse_color_shorthand_white() {
        let (r, g, b) = parse_color("#fff");
        assert!((r - 1.0).abs() < 1e-9);
        assert!((g - 1.0).abs() < 1e-9);
        assert!((b - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_color_gray() {
        let (r, g, b) = parse_color("#808080");
        let expected = 0x80 as f64 / 255.0;
        assert!((r - expected).abs() < 1e-9);
        assert!((g - expected).abs() < 1e-9);
        assert!((b - expected).abs() < 1e-9);
    }

    #[test]
    fn parse_color_invalid_returns_black() {
        assert_eq!(parse_color("invalid"), (0.0, 0.0, 0.0));
        assert_eq!(parse_color(""), (0.0, 0.0, 0.0));
        assert_eq!(parse_color("#GGGGGG"), (0.0, 0.0, 0.0));
        assert_eq!(parse_color("#12345"), (0.0, 0.0, 0.0)); // 5 chars
    }

    #[test]
    fn parse_color_lowercase() {
        let (r, g, b) = parse_color("#ff0000");
        assert!((r - 1.0).abs() < 1e-9);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
    }

    // ── draw_line ─────────────────────────────────────────────────────────────

    #[test]
    fn draw_line_contains_path_operators() {
        let ops = draw_line(10.0, 20.0, 100.0, 20.0, 1.0, "#000000");
        assert!(ops.contains(" m ") || ops.contains(" m\n") || ops.ends_with("m ") || ops.contains("m "), "deve conter moveto");
        assert!(ops.contains(" l ") || ops.contains(" l\n") || ops.contains("l S"), "deve conter lineto");
        assert!(ops.contains("S"), "deve conter stroke");
    }

    #[test]
    fn draw_line_wraps_in_save_restore() {
        let ops = draw_line(0.0, 0.0, 100.0, 0.0, 0.5, "#000000");
        assert!(ops.starts_with("q "), "deve iniciar com q (save state)");
        assert!(ops.trim_end().ends_with("Q"), "deve terminar com Q (restore state)");
    }

    #[test]
    fn draw_line_sets_color() {
        let ops = draw_line(0.0, 0.0, 50.0, 0.0, 1.0, "#FF0000");
        // Vermelho puro: RG com 1.0000 0.0000 0.0000
        assert!(ops.contains("1.0000 0.0000 0.0000 RG"), "deve definir cor vermelha com RG");
    }

    // ── draw_rect ─────────────────────────────────────────────────────────────

    #[test]
    fn draw_rect_contains_rect_and_stroke() {
        let border = Border { width: Some(1.0), color: Some("#000000".to_string()), style: None };
        let ops = draw_rect(10.0, 10.0, 100.0, 50.0, &border);
        assert!(ops.contains(" re ") || ops.contains(" re\n"), "deve conter operador re");
        assert!(ops.contains("S"), "deve conter stroke");
    }

    #[test]
    fn draw_rect_dashed_contains_dash_array() {
        let border = Border {
            width: Some(1.0),
            color: None,
            style: Some(BorderStyle::Dashed),
        };
        let ops = draw_rect(0.0, 0.0, 100.0, 50.0, &border);
        assert!(ops.contains("[6 3] 0 d"), "borda dashed deve conter array de dash");
    }

    #[test]
    fn draw_rect_dotted_contains_dot_array() {
        let border = Border {
            width: Some(1.0),
            color: None,
            style: Some(BorderStyle::Dotted),
        };
        let ops = draw_rect(0.0, 0.0, 100.0, 50.0, &border);
        assert!(ops.contains("[1 3] 0 d"), "borda dotted deve conter array de ponto");
    }

    #[test]
    fn draw_rect_solid_contains_empty_dash_array() {
        let border = Border {
            width: Some(1.0),
            color: None,
            style: Some(BorderStyle::Solid),
        };
        let ops = draw_rect(0.0, 0.0, 100.0, 50.0, &border);
        assert!(ops.contains("[] 0 d"), "borda solid deve ter array de dash vazio");
    }

    // ── draw_answer_lines ─────────────────────────────────────────────────────

    #[test]
    fn draw_answer_lines_count() {
        let ops = draw_answer_lines(72.0, 700.0, 400.0, 3, DEFAULT_LINE_SPACING);
        // Cada linha gera exatamente um "S Q" no final
        let count = ops.matches("S Q").count();
        assert_eq!(count, 3, "deve gerar exatamente 3 linhas");
    }

    #[test]
    fn draw_answer_lines_zero_count() {
        let ops = draw_answer_lines(72.0, 700.0, 400.0, 0, DEFAULT_LINE_SPACING);
        assert!(ops.is_empty(), "count=0 deve retornar string vazia");
    }

    #[test]
    fn draw_answer_lines_uses_light_gray() {
        let ops = draw_answer_lines(72.0, 700.0, 400.0, 1, DEFAULT_LINE_SPACING);
        let (r, g, b) = parse_color(DEFAULT_ANSWER_LINE_COLOR);
        let color_fragment = format!("{:.4} {:.4} {:.4} RG", r, g, b);
        assert!(ops.contains(&color_fragment), "linhas devem usar cor cinza claro");
    }

    // ── draw_answer_box ───────────────────────────────────────────────────────

    #[test]
    fn draw_answer_box_produces_rect() {
        let ops = draw_answer_box(72.0, 600.0, 400.0, 80.0, None);
        assert!(ops.contains("re"), "answer box deve conter operador re");
        assert!(ops.contains("S"), "answer box deve conter stroke");
    }

    #[test]
    fn draw_answer_box_with_custom_border() {
        let border = Border {
            width: Some(2.0),
            color: Some("#FF0000".to_string()),
            style: Some(BorderStyle::Dashed),
        };
        let ops = draw_answer_box(72.0, 600.0, 400.0, 80.0, Some(&border));
        assert!(ops.contains("2.0000 w"), "deve usar espessura especificada");
        assert!(ops.contains("[6 3] 0 d"), "deve usar estilo dashed");
    }

    // ── draw_answer_grid ──────────────────────────────────────────────────────

    #[test]
    fn draw_answer_grid_line_count() {
        // 2 rows + 3 cols → 3 linhas horizontais + 4 linhas verticais = 7 linhas
        let ops = draw_answer_grid(72.0, 600.0, 300.0, 80.0, 2, 3);
        let count = ops.matches("S Q").count();
        assert_eq!(count, 7, "2x3 grid deve ter 3+4=7 linhas");
    }

    #[test]
    fn draw_answer_grid_zero_rows_empty() {
        assert!(draw_answer_grid(0.0, 0.0, 100.0, 50.0, 0, 3).is_empty());
        assert!(draw_answer_grid(0.0, 0.0, 100.0, 50.0, 3, 0).is_empty());
    }

    #[test]
    fn draw_answer_grid_1x1() {
        // 1 row + 1 col → 2 horizontais + 2 verticais = 4 linhas
        let ops = draw_answer_grid(0.0, 100.0, 100.0, 50.0, 1, 1);
        let count = ops.matches("S Q").count();
        assert_eq!(count, 4);
    }
}
