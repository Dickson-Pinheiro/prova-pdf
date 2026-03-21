/// Sistema de cores do ExamPDF.
///
/// Suporta os formatos CSS:
///   - `#RRGGBB` / `#RGB`         — hexadecimal sRGB
///   - `rgb(r, g, b)`              — componentes 0–255 ou 0%–100%
///   - `rgba(r, g, b, a)`          — idem + alpha 0–1
///   - `oklch(L C H)`              — CSS Color 4, perceptualmente uniforme
///   - `oklch(L C H / alpha)`      — idem com canal alpha
///
/// Internamente as cores são armazenadas em **sRGB linear** (sem gamma).
/// Isso permite que operações matemáticas (blending, conversão de espaço de cor)
/// sejam corretas. Para emissão PDF, use `Color::to_srgb()` que aplica gamma.
///
/// # Modo preto e branco
/// `ColorResolver::resolve` converte qualquer cor para escala de cinza quando
/// `bw_mode = true`. A estratégia depende da origem:
/// - Cores **OKLCH**: usa diretamente o canal `L` (já perceptualmente uniforme).
/// - Cores **hex / rgb**: usa luminância Rec. 709 sobre sRGB linear.

use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Erros
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ColorError {
    #[error("formato de cor desconhecido: {0:?}")]
    UnknownFormat(String),
    #[error("valor inválido em {format}: {detail}")]
    ParseError { format: &'static str, detail: String },
}

// ─────────────────────────────────────────────────────────────────────────────
// Origem da cor — usada para escolher a estratégia de conversão P&B
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ColorOrigin {
    /// Cor definida como hex (`#RRGGBB`) ou `rgb(...)`.
    Srgb,
    /// Cor definida como `oklch(L C H)`. Guarda `L` (0.0–1.0) para P&B direto.
    Oklch { l: f64 },
}

// ─────────────────────────────────────────────────────────────────────────────
// Struct Color
// ─────────────────────────────────────────────────────────────────────────────

/// Cor armazenada em sRGB **linear** (sem gamma), alpha 0.0–1.0.
#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    /// Vermelho linear 0.0–1.0.
    pub r: f64,
    /// Verde linear 0.0–1.0.
    pub g: f64,
    /// Azul linear 0.0–1.0.
    pub b: f64,
    /// Transparência 0.0 (transparente) – 1.0 (opaco).
    pub alpha: f64,
    /// Formato de origem (afeta estratégia de conversão P&B).
    pub origin: ColorOrigin,
}

impl Color {
    // ── Construtores ──────────────────────────────────────────────────────────

    pub fn black() -> Self {
        Self { r: 0.0, g: 0.0, b: 0.0, alpha: 1.0, origin: ColorOrigin::Srgb }
    }

    pub fn white() -> Self {
        Self { r: 1.0, g: 1.0, b: 1.0, alpha: 1.0, origin: ColorOrigin::Srgb }
    }

    /// `level` em sRGB linear 0.0–1.0.
    pub fn gray(level: f64) -> Self {
        Self { r: level, g: level, b: level, alpha: 1.0, origin: ColorOrigin::Srgb }
    }

    pub fn transparent() -> Self {
        Self { r: 0.0, g: 0.0, b: 0.0, alpha: 0.0, origin: ColorOrigin::Srgb }
    }

    // ── Parsing ───────────────────────────────────────────────────────────────

    /// Parseia qualquer formato de cor suportado.
    ///
    /// Aceita (case-insensitive, espaços externos ignorados):
    /// `#RRGGBB`, `#RGB`, `rgb(...)`, `rgba(...)`, `oklch(...)`.
    pub fn from_str(s: &str) -> Result<Self, ColorError> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();

        if lower.starts_with('#') {
            parse_hex(s)
        } else if lower.starts_with("rgba(") {
            parse_rgb_fn(s, true)
        } else if lower.starts_with("rgb(") {
            parse_rgb_fn(s, false)
        } else if lower.starts_with("oklch(") {
            parse_oklch(s)
        } else {
            Err(ColorError::UnknownFormat(s.to_string()))
        }
    }

    // ── Saída ─────────────────────────────────────────────────────────────────

    /// Retorna `(r, g, b)` em sRGB **gamma-corrigido** (0.0–1.0), pronto para
    /// emissão nos operadores PDF `rg` / `RG`.
    pub fn to_srgb(&self) -> (f64, f64, f64) {
        (linear_to_srgb(self.r), linear_to_srgb(self.g), linear_to_srgb(self.b))
    }

    /// Converte para escala de cinza (0.0 = preto, 1.0 = branco) em sRGB gamma.
    ///
    /// - Origem OKLCH: usa `L` diretamente (já perceptualmente uniforme).
    /// - Origem hex/rgb: luminância Rec. 709 sobre sRGB linear.
    pub fn to_grayscale(&self) -> f64 {
        match self.origin {
            ColorOrigin::Oklch { l } => {
                // L ∈ [0,1] em OKLab é a percepção de brilho.
                // Para sRGB-gamma: linear = L^3, depois gamma-encode.
                linear_to_srgb((l * l * l).clamp(0.0, 1.0))
            }
            ColorOrigin::Srgb => {
                // Luminância Rec. 709 sobre linear sRGB.
                let lum = 0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b;
                linear_to_srgb(lum.clamp(0.0, 1.0))
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Funções de gamma
// ─────────────────────────────────────────────────────────────────────────────

/// sRGB → linear sRGB (gamma decode, IEC 61966-2-1).
pub fn srgb_to_linear(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c == 0.0 { return 0.0; }
    if c == 1.0 { return 1.0; }
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Linear sRGB → sRGB (gamma encode, IEC 61966-2-1).
pub fn linear_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c == 0.0 { return 0.0; }
    if c == 1.0 { return 1.0; }
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsing: hex
// ─────────────────────────────────────────────────────────────────────────────

fn parse_hex(s: &str) -> Result<Color, ColorError> {
    let hex = s.trim().trim_start_matches('#');
    let (r_byte, g_byte, b_byte) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16)
                .map_err(|_| ColorError::ParseError { format: "hex", detail: hex.to_string() })?;
            let g = u8::from_str_radix(&hex[2..4], 16)
                .map_err(|_| ColorError::ParseError { format: "hex", detail: hex.to_string() })?;
            let b = u8::from_str_radix(&hex[4..6], 16)
                .map_err(|_| ColorError::ParseError { format: "hex", detail: hex.to_string() })?;
            (r, g, b)
        }
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16)
                .map_err(|_| ColorError::ParseError { format: "hex", detail: hex.to_string() })?;
            let g = u8::from_str_radix(&hex[1..2], 16)
                .map_err(|_| ColorError::ParseError { format: "hex", detail: hex.to_string() })?;
            let b = u8::from_str_radix(&hex[2..3], 16)
                .map_err(|_| ColorError::ParseError { format: "hex", detail: hex.to_string() })?;
            (r * 17, g * 17, b * 17)
        }
        _ => return Err(ColorError::ParseError { format: "hex", detail: format!("comprimento inválido: {}", hex.len()) }),
    };
    Ok(Color {
        r: srgb_to_linear(r_byte as f64 / 255.0),
        g: srgb_to_linear(g_byte as f64 / 255.0),
        b: srgb_to_linear(b_byte as f64 / 255.0),
        alpha: 1.0,
        origin: ColorOrigin::Srgb,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsing: rgb() / rgba()
// ─────────────────────────────────────────────────────────────────────────────

fn parse_rgb_fn(s: &str, has_alpha: bool) -> Result<Color, ColorError> {
    // Extrai conteúdo dentro dos parênteses
    let inner = s
        .find('(').and_then(|i| s.rfind(')').map(|j| &s[i + 1..j]))
        .ok_or_else(|| ColorError::ParseError { format: "rgb", detail: "parênteses ausentes".to_string() })?;

    let parts: Vec<&str> = inner.split(',').collect();
    let expected = if has_alpha { 4 } else { 3 };
    if parts.len() != expected {
        return Err(ColorError::ParseError {
            format: "rgb",
            detail: format!("esperados {} componentes, encontrados {}", expected, parts.len()),
        });
    }

    let parse_component = |p: &str| -> Result<f64, ColorError> {
        let p = p.trim();
        if let Some(pct) = p.strip_suffix('%') {
            pct.trim().parse::<f64>()
                .map(|v| (v / 100.0).clamp(0.0, 1.0))
                .map_err(|_| ColorError::ParseError { format: "rgb", detail: p.to_string() })
        } else {
            p.parse::<f64>()
                .map(|v| (v / 255.0).clamp(0.0, 1.0))
                .map_err(|_| ColorError::ParseError { format: "rgb", detail: p.to_string() })
        }
    };

    let r = srgb_to_linear(parse_component(parts[0])?);
    let g = srgb_to_linear(parse_component(parts[1])?);
    let b = srgb_to_linear(parse_component(parts[2])?);
    let alpha = if has_alpha {
        let p = parts[3].trim();
        if let Some(pct) = p.strip_suffix('%') {
            pct.trim().parse::<f64>()
                .map(|v| (v / 100.0).clamp(0.0, 1.0))
                .map_err(|_| ColorError::ParseError { format: "rgba", detail: p.to_string() })?
        } else {
            p.parse::<f64>()
                .map(|v| v.clamp(0.0, 1.0))
                .map_err(|_| ColorError::ParseError { format: "rgba", detail: p.to_string() })?
        }
    } else {
        1.0
    };

    Ok(Color { r, g, b, alpha, origin: ColorOrigin::Srgb })
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsing: oklch()
// ─────────────────────────────────────────────────────────────────────────────

fn parse_oklch(s: &str) -> Result<Color, ColorError> {
    // Extrai conteúdo entre parênteses
    let inner = s
        .find('(').and_then(|i| s.rfind(')').map(|j| &s[i + 1..j]))
        .ok_or_else(|| ColorError::ParseError { format: "oklch", detail: "parênteses ausentes".to_string() })?;

    // Separa alpha pelo '/'
    let (main_part, alpha_part) = if let Some(slash) = inner.find('/') {
        (&inner[..slash], Some(inner[slash + 1..].trim()))
    } else {
        (inner, None)
    };

    let tokens: Vec<&str> = main_part.split_whitespace().collect();
    if tokens.len() != 3 {
        return Err(ColorError::ParseError {
            format: "oklch",
            detail: format!("esperados 3 componentes (L C H), encontrados {}", tokens.len()),
        });
    }

    // L: 0.0–1.0 ou 0%–100%
    let l = parse_oklch_l(tokens[0])?;
    // C: número ≥ 0
    let c = parse_f64_or_none(tokens[1])
        .ok_or_else(|| ColorError::ParseError { format: "oklch", detail: format!("C inválido: {}", tokens[1]) })?
        .max(0.0);
    // H: graus (0–360), sufixo "deg" opcional, "none" = 0
    let h = parse_oklch_h(tokens[2])?;

    let alpha = match alpha_part {
        None => 1.0,
        Some(p) => {
            if let Some(pct) = p.strip_suffix('%') {
                pct.trim().parse::<f64>()
                    .map(|v| (v / 100.0).clamp(0.0, 1.0))
                    .map_err(|_| ColorError::ParseError { format: "oklch", detail: format!("alpha inválido: {p}") })?
            } else {
                p.parse::<f64>()
                    .map(|v| v.clamp(0.0, 1.0))
                    .map_err(|_| ColorError::ParseError { format: "oklch", detail: format!("alpha inválido: {p}") })?
            }
        }
    };

    let (r, g, b) = oklch_to_linear_srgb(l, c, h);
    let (r, g, b) = clamp_to_srgb_gamut_oklch(r, g, b, l, c, h);

    Ok(Color { r, g, b, alpha, origin: ColorOrigin::Oklch { l } })
}

fn parse_oklch_l(s: &str) -> Result<f64, ColorError> {
    if let Some(pct) = s.strip_suffix('%') {
        pct.trim().parse::<f64>()
            .map(|v| (v / 100.0).clamp(0.0, 1.0))
            .map_err(|_| ColorError::ParseError { format: "oklch", detail: format!("L inválido: {s}") })
    } else {
        s.parse::<f64>()
            .map(|v| v.clamp(0.0, 1.0))
            .map_err(|_| ColorError::ParseError { format: "oklch", detail: format!("L inválido: {s}") })
    }
}

fn parse_oklch_h(s: &str) -> Result<f64, ColorError> {
    let s_lower = s.to_ascii_lowercase();
    if s_lower == "none" { return Ok(0.0); }
    let s_num = s_lower.trim_end_matches("deg");
    s_num.parse::<f64>()
        .map_err(|_| ColorError::ParseError { format: "oklch", detail: format!("H inválido: {s}") })
}

fn parse_f64_or_none(s: &str) -> Option<f64> {
    if s.to_ascii_lowercase() == "none" { return Some(0.0); }
    s.parse::<f64>().ok()
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversão OKLCH → sRGB linear
// ─────────────────────────────────────────────────────────────────────────────

/// Converte OKLCH para sRGB linear (pode produzir valores fora de [0,1]).
///
/// Pipeline: OKLCH → OKLab → LMS cúbico → Linear sRGB
/// Matrizes da especificação CSS Color 4 (Björn Ottosson, 2020).
pub fn oklch_to_linear_srgb(l: f64, c: f64, h_deg: f64) -> (f64, f64, f64) {
    // 1. OKLCH → OKLab
    let h_rad = h_deg * std::f64::consts::PI / 180.0;
    let a = c * h_rad.cos();
    let b = c * h_rad.sin();

    // 2. OKLab → LMS (cubo)
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    let lms_l = l_ * l_ * l_;
    let lms_m = m_ * m_ * m_;
    let lms_s = s_ * s_ * s_;

    // 3. LMS → Linear sRGB
    let r =  4.0767416621 * lms_l - 3.3077115913 * lms_m + 0.2309699292 * lms_s;
    let g = -1.2684380046 * lms_l + 2.6097574011 * lms_m - 0.3413193965 * lms_s;
    let b = -0.0041960863 * lms_l - 0.7034186147 * lms_m + 1.7076147010 * lms_s;

    (r, g, b)
}

/// Converte sRGB linear para OKLCH (inverso de oklch_to_linear_srgb).
fn linear_srgb_to_oklch(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    // Linear sRGB → LMS
    let lms_l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let lms_m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let lms_s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;

    // LMS → OKLab (raiz cúbica)
    let l_ = lms_l.cbrt();
    let m_ = lms_m.cbrt();
    let s_ = lms_s.cbrt();

    let l = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
    let a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
    let b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;

    let c = (a * a + b * b).sqrt();
    let h = b.atan2(a).to_degrees().rem_euclid(360.0);

    (l, c, h)
}

/// Gamut mapping: redução iterativa de chroma até entrar no gamut sRGB.
///
/// Se os canais já estiverem em [0, 1], retorna sem alteração.
/// Caso contrário, reduz C em 5% por iteração (até 20 vezes),
/// depois aplica clamp simples.
fn clamp_to_srgb_gamut_oklch(
    r: f64, g: f64, b: f64,
    l: f64, c: f64, h: f64,
) -> (f64, f64, f64) {
    const EPS: f64 = 0.0001;
    // Verificação rápida: já está no gamut?
    if r >= -EPS && r <= 1.0 + EPS
        && g >= -EPS && g <= 1.0 + EPS
        && b >= -EPS && b <= 1.0 + EPS
    {
        return (r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
    }

    // Redução iterativa de chroma
    let mut c_cur = c;
    for _ in 0..20 {
        c_cur *= 0.95;
        if c_cur < EPS { break; }
        let (r2, g2, b2) = oklch_to_linear_srgb(l, c_cur, h);
        if r2 >= -EPS && r2 <= 1.0 + EPS
            && g2 >= -EPS && g2 <= 1.0 + EPS
            && b2 >= -EPS && b2 <= 1.0 + EPS
        {
            return (r2.clamp(0.0, 1.0), g2.clamp(0.0, 1.0), b2.clamp(0.0, 1.0));
        }
    }

    // Fallback: clamp simples
    (r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
}

// ─────────────────────────────────────────────────────────────────────────────
// PdfColor — representação de cor para emissão no content stream PDF
// ─────────────────────────────────────────────────────────────────────────────

/// Cor resolvida pronta para emissão como operadores PDF.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfColor {
    /// DeviceRGB — valores em sRGB gamma-encoded 0.0–1.0.
    Rgb(f64, f64, f64),
    /// DeviceGray — valor 0.0 (preto) – 1.0 (branco).
    Gray(f64),
}

impl PdfColor {
    /// Operador de cor de preenchimento (texto, formas): `rg` ou `g`.
    pub fn to_fill_ops(&self) -> String {
        match self {
            PdfColor::Rgb(r, g, b) => format!("{r:.4} {g:.4} {b:.4} rg"),
            PdfColor::Gray(g)      => format!("{g:.4} g"),
        }
    }

    /// Operador de cor de contorno (bordas, linhas): `RG` ou `G`.
    pub fn to_stroke_ops(&self) -> String {
        match self {
            PdfColor::Rgb(r, g, b) => format!("{r:.4} {g:.4} {b:.4} RG"),
            PdfColor::Gray(g)      => format!("{g:.4} G"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ColorResolver — ponto central de resolução de cor
// ─────────────────────────────────────────────────────────────────────────────

/// Converte uma `Color` para `PdfColor`, aplicando o modo P&B quando necessário.
#[derive(Debug, Clone)]
pub struct ColorResolver {
    /// Se `true`, todas as cores são convertidas para escala de cinza.
    pub bw_mode: bool,
}

impl ColorResolver {
    pub fn new(bw_mode: bool) -> Self {
        Self { bw_mode }
    }

    /// Resolve uma `Color` para `PdfColor` para emissão PDF.
    pub fn resolve(&self, color: &Color) -> PdfColor {
        if self.bw_mode {
            PdfColor::Gray(color.to_grayscale())
        } else {
            let (r, g, b) = color.to_srgb();
            PdfColor::Rgb(r, g, b)
        }
    }

    /// Conveniência: parseia e resolve em um passo.
    /// Retorna preto em caso de string inválida.
    pub fn resolve_str(&self, s: &str) -> PdfColor {
        let color = Color::from_str(s).unwrap_or_else(|_| Color::black());
        self.resolve(&color)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Testes
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool { (a - b).abs() < 1e-3 }
    fn approx_tight(a: f64, b: f64) -> bool { (a - b).abs() < 1e-9 }

    // ── sRGB gamma ────────────────────────────────────────────────────────────

    #[test]
    fn gamma_roundtrip() {
        for v in [0.0, 0.1, 0.5, 0.8, 1.0] {
            let rt = linear_to_srgb(srgb_to_linear(v));
            assert!((rt - v).abs() < 1e-9, "roundtrip falhou para {v}: {rt}");
        }
    }

    #[test]
    fn gamma_black_white() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert_eq!(srgb_to_linear(1.0), 1.0);
        assert_eq!(linear_to_srgb(0.0), 0.0);
        assert_eq!(linear_to_srgb(1.0), 1.0);
    }

    // ── parse_hex ─────────────────────────────────────────────────────────────

    #[test]
    fn hex_black() {
        let c = Color::from_str("#000000").unwrap();
        assert_eq!(c.r, 0.0); assert_eq!(c.g, 0.0); assert_eq!(c.b, 0.0);
        assert_eq!(c.alpha, 1.0);
        assert_eq!(c.origin, ColorOrigin::Srgb);
    }

    #[test]
    fn hex_white() {
        let c = Color::from_str("#FFFFFF").unwrap();
        assert!(approx_tight(c.r, 1.0));
        assert!(approx_tight(c.g, 1.0));
        assert!(approx_tight(c.b, 1.0));
    }

    #[test]
    fn hex_red() {
        let c = Color::from_str("#FF0000").unwrap();
        assert!(approx_tight(c.r, 1.0)); // linear(1.0) = 1.0
        assert_eq!(c.g, 0.0);
        assert_eq!(c.b, 0.0);
    }

    #[test]
    fn hex_mid_gray_linear() {
        // #808080 em sRGB-gamma ≈ 0.502; em linear ≈ 0.2158
        let c = Color::from_str("#808080").unwrap();
        let expected_lin = srgb_to_linear(0x80 as f64 / 255.0);
        assert!((c.r - expected_lin).abs() < 1e-9);
        assert!((c.g - expected_lin).abs() < 1e-9);
    }

    #[test]
    fn hex_shorthand() {
        let long  = Color::from_str("#FF0000").unwrap();
        let short = Color::from_str("#F00").unwrap();
        assert!(approx_tight(long.r, short.r));
        assert!(approx_tight(long.g, short.g));
    }

    #[test]
    fn hex_lowercase() {
        let c = Color::from_str("#ff0000").unwrap();
        assert!(approx_tight(c.r, 1.0));
    }

    #[test]
    fn hex_invalid() {
        assert!(Color::from_str("#GGGGGG").is_err());
        assert!(Color::from_str("#12345").is_err());
        assert!(Color::from_str("invalid").is_err());
    }

    // ── to_srgb (round-trip) ──────────────────────────────────────────────────

    #[test]
    fn to_srgb_hex_roundtrip() {
        // Vermelho #FF0000: to_srgb() deve retornar (1.0, 0.0, 0.0)
        let c = Color::from_str("#FF0000").unwrap();
        let (r, g, b) = c.to_srgb();
        assert!(approx_tight(r, 1.0));
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn to_srgb_gray_roundtrip() {
        let byte = 0x80_u8;
        let srgb_in = byte as f64 / 255.0;
        let c = Color::from_str(&format!("#{byte:02X}{byte:02X}{byte:02X}")).unwrap();
        let (r, _, _) = c.to_srgb();
        assert!((r - srgb_in).abs() < 1e-6, "roundtrip: {r} != {srgb_in}");
    }

    // ── parse_rgb() ───────────────────────────────────────────────────────────

    #[test]
    fn rgb_fn_red() {
        let c = Color::from_str("rgb(255, 0, 0)").unwrap();
        assert!(approx_tight(c.r, 1.0));
        assert_eq!(c.g, 0.0);
        assert_eq!(c.b, 0.0);
    }

    #[test]
    fn rgb_fn_percent() {
        let c = Color::from_str("rgb(100%, 0%, 0%)").unwrap();
        assert!(approx_tight(c.r, 1.0));
    }

    #[test]
    fn rgba_fn_alpha() {
        let c = Color::from_str("rgba(255, 255, 255, 0.5)").unwrap();
        assert!(approx_tight(c.alpha, 0.5));
    }

    #[test]
    fn rgb_wrong_components() {
        assert!(Color::from_str("rgb(255, 0)").is_err());
    }

    // ── parse_oklch() ─────────────────────────────────────────────────────────

    #[test]
    fn oklch_black() {
        let c = Color::from_str("oklch(0.0 0.0 0)").unwrap();
        assert!(approx(c.r, 0.0));
        assert!(approx(c.g, 0.0));
        assert!(approx(c.b, 0.0));
    }

    #[test]
    fn oklch_white() {
        let c = Color::from_str("oklch(1.0 0.0 0)").unwrap();
        assert!(approx(c.r, 1.0), "r={}", c.r);
        assert!(approx(c.g, 1.0), "g={}", c.g);
        assert!(approx(c.b, 1.0), "b={}", c.b);
    }

    #[test]
    fn oklch_gray_neutral() {
        // L=0.5, C=0 → todos os canais iguais
        let c = Color::from_str("oklch(0.5 0.0 0)").unwrap();
        assert!((c.r - c.g).abs() < 1e-9, "r={} g={}", c.r, c.g);
        assert!((c.r - c.b).abs() < 1e-9, "r={} b={}", c.r, c.b);
    }

    #[test]
    fn oklch_red_reference() {
        // oklch(0.6279554 0.2576965 29.234) ≈ #FF0000 (referência CSS Color 4)
        let c = Color::from_str("oklch(0.6279554 0.2576965 29.234)").unwrap();
        let (r, g, b) = c.to_srgb();
        assert!(approx(r, 1.0), "r esperado ≈1.0, obtido {r}");
        assert!(approx(g, 0.0), "g esperado ≈0.0, obtido {g}");
        assert!(approx(b, 0.0), "b esperado ≈0.0, obtido {b}");
    }

    #[test]
    fn oklch_percent_l() {
        let a = Color::from_str("oklch(70% 0.15 200)").unwrap();
        let b = Color::from_str("oklch(0.7 0.15 200)").unwrap();
        assert!((a.r - b.r).abs() < 1e-9);
    }

    #[test]
    fn oklch_with_alpha() {
        let c = Color::from_str("oklch(0.5 0.0 0 / 0.5)").unwrap();
        assert!(approx_tight(c.alpha, 0.5));
    }

    #[test]
    fn oklch_stores_origin() {
        let c = Color::from_str("oklch(0.7 0.15 200)").unwrap();
        assert!(matches!(c.origin, ColorOrigin::Oklch { l } if (l - 0.7).abs() < 1e-9));
    }

    #[test]
    fn oklch_gamut_high_chroma() {
        // C=0.4 pode estar fora do gamut sRGB — resultado deve estar em [0,1]
        let c = Color::from_str("oklch(0.7 0.4 150)").unwrap();
        assert!(c.r >= 0.0 && c.r <= 1.0, "r={}", c.r);
        assert!(c.g >= 0.0 && c.g <= 1.0, "g={}", c.g);
        assert!(c.b >= 0.0 && c.b <= 1.0, "b={}", c.b);
    }

    // ── to_grayscale ──────────────────────────────────────────────────────────

    #[test]
    fn grayscale_black() {
        assert_eq!(Color::black().to_grayscale(), 0.0);
    }

    #[test]
    fn grayscale_white() {
        assert_eq!(Color::white().to_grayscale(), 1.0);
    }

    #[test]
    fn grayscale_hex_red_uses_rec709() {
        // #FF0000 → lum = 0.2126 (linear) → sRGB ≈ 0.5013
        let c = Color::from_str("#FF0000").unwrap();
        let gray = c.to_grayscale();
        let expected = linear_to_srgb(0.2126);
        assert!(approx(gray, expected), "gray={gray} expected={expected}");
    }

    #[test]
    fn grayscale_oklch_uses_l_channel() {
        // oklch(0.7 0.3 200) → grayscale deve usar L=0.7
        let c = Color::from_str("oklch(0.7 0.3 200)").unwrap();
        let gray = c.to_grayscale();
        let expected = linear_to_srgb((0.7_f64).powi(3));
        assert!(approx(gray, expected), "gray={gray} expected={expected}");
    }

    // ── PdfColor ──────────────────────────────────────────────────────────────

    #[test]
    fn pdfcolor_rgb_fill_ops() {
        assert_eq!(PdfColor::Rgb(1.0, 0.0, 0.0).to_fill_ops(),   "1.0000 0.0000 0.0000 rg");
        assert_eq!(PdfColor::Rgb(1.0, 0.0, 0.0).to_stroke_ops(), "1.0000 0.0000 0.0000 RG");
    }

    #[test]
    fn pdfcolor_gray_ops() {
        assert_eq!(PdfColor::Gray(0.5).to_fill_ops(),   "0.5000 g");
        assert_eq!(PdfColor::Gray(0.5).to_stroke_ops(), "0.5000 G");
    }

    // ── ColorResolver ─────────────────────────────────────────────────────────

    #[test]
    fn resolver_color_mode() {
        let r = ColorResolver::new(false);
        let c = Color::from_str("#FF0000").unwrap();
        assert_eq!(r.resolve(&c), PdfColor::Rgb(1.0, 0.0, 0.0));
    }

    #[test]
    fn resolver_bw_mode_hex() {
        let r = ColorResolver::new(true);
        let c = Color::from_str("#FF0000").unwrap();
        let gray = c.to_grayscale();
        assert_eq!(r.resolve(&c), PdfColor::Gray(gray));
    }

    #[test]
    fn resolver_bw_mode_oklch() {
        let r = ColorResolver::new(true);
        let c = Color::from_str("oklch(0.7 0.3 30)").unwrap();
        let gray = c.to_grayscale();
        // Deve usar L channel
        assert_eq!(r.resolve(&c), PdfColor::Gray(gray));
    }

    #[test]
    fn resolver_str_oklch() {
        let r = ColorResolver::new(false);
        let pc = r.resolve_str("oklch(1.0 0.0 0)");
        if let PdfColor::Rgb(rv, g, b) = pc {
            assert!(approx(rv, 1.0), "r={rv}");
            assert!(approx(g,  1.0), "g={g}");
            assert!(approx(b,  1.0), "b={b}");
        } else {
            panic!("esperava Rgb");
        }
    }

    #[test]
    fn resolver_str_invalid_returns_black() {
        let r = ColorResolver::new(false);
        assert_eq!(r.resolve_str("invalid"), PdfColor::Rgb(0.0, 0.0, 0.0));
    }
}
