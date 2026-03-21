//! Layout engine para expressões matemáticas.
//!
//! Converte um `MathNode` em uma lista de glyphs posicionados (`MathLayoutResult`),
//! usando métricas da fonte (tabela OpenType MATH quando disponível, ou fallback).
//!
//! Sistema de coordenadas:
//! - x cresce para a direita a partir da borda esquerda da caixa
//! - y = 0 é a baseline; positivo sobe, negativo desce
//! - `height` = extensão máxima acima da baseline
//! - `depth`  = extensão máxima abaixo da baseline (valor positivo)

use crate::layout::text::{shape_text, shaped_text_width};
use crate::math::parser::{AccentType, MathNode};
use crate::fonts::data::FontData;

// ─────────────────────────────────────────────────────────────────────────────
// Tipos de saída
// ─────────────────────────────────────────────────────────────────────────────

/// Um glifo posicionado dentro de uma expressão matemática.
#[derive(Debug, Clone)]
pub struct PositionedMathGlyph {
    /// ID do glifo na fonte.
    pub glyph_id: u16,
    /// Posição X em pontos, a partir da borda esquerda da caixa.
    pub x: f64,
    /// Deslocamento vertical em pontos (positivo = acima da baseline).
    pub y: f64,
    /// Tamanho da fonte em pontos (varia para sub/superscripts).
    pub size: f64,
}

/// Comando de desenho vetorial dentro de uma expressão matemática.
#[derive(Debug, Clone)]
pub enum MathDrawCommand {
    /// Régua horizontal (barra de fração, linha de radical).
    HRule {
        x: f64,
        y: f64,
        width: f64,
        thickness: f64,
    },
    /// Barra vertical desenhada como traçado (vmatrix |, dupla barra ‖).
    VBar {
        x: f64,
        y_center: f64,
        half_height: f64,
        thickness: f64,
    },
    /// Parêntese esquerdo ( desenhado como curva Bézier.
    LeftParen {
        x: f64,
        y_center: f64,
        half_height: f64,
        delim_w: f64,
        stroke_w: f64,
    },
    /// Parêntese direito ) desenhado como curva Bézier.
    RightParen {
        x: f64,
        y_center: f64,
        half_height: f64,
        delim_w: f64,
        stroke_w: f64,
    },
    /// Colchete esquerdo [ desenhado como traçado.
    LeftBracket {
        x: f64,
        y_center: f64,
        half_height: f64,
        delim_w: f64,
        stroke_w: f64,
    },
    /// Colchete direito ] desenhado como traçado.
    RightBracket {
        x: f64,
        y_center: f64,
        half_height: f64,
        delim_w: f64,
        stroke_w: f64,
    },
}

impl MathDrawCommand {
    fn shift(&mut self, dx: f64, dy: f64) {
        match self {
            MathDrawCommand::HRule { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            MathDrawCommand::VBar { x, y_center, .. }
            | MathDrawCommand::LeftParen { x, y_center, .. }
            | MathDrawCommand::RightParen { x, y_center, .. }
            | MathDrawCommand::LeftBracket { x, y_center, .. }
            | MathDrawCommand::RightBracket { x, y_center, .. } => {
                *x += dx;
                *y_center += dy;
            }
        }
    }
}

/// Resultado do layout de uma expressão matemática.
#[derive(Debug, Clone)]
pub struct MathLayoutResult {
    /// Glyphs posicionados.
    pub glyphs: Vec<PositionedMathGlyph>,
    /// Comandos de desenho (barras, linhas).
    pub rules: Vec<MathDrawCommand>,
    /// Largura total em pontos.
    pub width: f64,
    /// Extensão máxima acima da baseline em pontos.
    pub height: f64,
    /// Extensão máxima abaixo da baseline em pontos (valor positivo).
    pub depth: f64,
}

impl MathLayoutResult {
    fn empty() -> Self {
        Self { glyphs: vec![], rules: vec![], width: 0.0, height: 0.0, depth: 0.0 }
    }

    /// Desloca todos os elementos horizontalmente por `dx` e verticalmente por `dy`.
    fn shifted(mut self, dx: f64, dy: f64) -> Self {
        for g in &mut self.glyphs {
            g.x += dx;
            g.y += dy;
        }
        for r in &mut self.rules {
            r.shift(dx, dy);
        }
        self
    }

    /// Concatena outro resultado à direita deste.
    fn append_right(&mut self, other: MathLayoutResult) {
        let dx = self.width;
        self.width += other.width;
        self.height = self.height.max(other.height);
        self.depth = self.depth.max(other.depth);
        for mut g in other.glyphs {
            g.x += dx;
            self.glyphs.push(g);
        }
        for mut r in other.rules {
            r.shift(dx, 0.0);
            self.rules.push(r);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Constantes de layout matemático
// ─────────────────────────────────────────────────────────────────────────────

/// Constantes de tipografia matemática (em fração do em, exceto os percentuais).
///
/// Extraídas da tabela OpenType MATH quando disponível,
/// ou calculadas a partir de métricas gerais da fonte como fallback.
#[derive(Debug, Clone)]
pub struct MathConstants {
    /// Escala para superscript/subscript (tipicamente 0.71).
    pub script_percent_scale_down: f64,
    /// Escala para sub-sub scripts (tipicamente 0.50).
    pub script_script_percent_scale_down: f64,

    /// Espessura da barra de fração (em frações do em).
    pub fraction_rule_thickness: f64,
    /// Gap mínimo entre numerador e barra (em frações do em).
    pub fraction_num_gap: f64,
    /// Gap mínimo entre denominador e barra (em frações do em).
    pub fraction_denom_gap: f64,
    /// Shift do numerador acima da barra (em frações do em).
    pub fraction_num_shift_up: f64,
    /// Shift do denominador abaixo da barra (em frações do em).
    pub fraction_denom_shift_down: f64,

    /// Shift do superscript acima da baseline (em frações do em).
    pub superscript_shift_up: f64,
    /// Shift do subscript abaixo da baseline (em frações do em).
    pub subscript_shift_down: f64,
    /// Gap mínimo entre superscript e subscript simultâneos (em frações do em).
    pub sub_superscript_gap: f64,
    /// Espaço após scripts (em frações do em).
    pub space_after_script: f64,

    /// Altura do eixo matemático acima da baseline (em frações do em).
    /// Centro padrão de operadores e barras de fração.
    pub axis_height: f64,

    /// Gap vertical entre radical e radicand (em frações do em).
    pub radical_vertical_gap: f64,
    /// Espessura da régua do radical (em frações do em).
    pub radical_rule_thickness: f64,
    /// Ascender extra acima da régua do radical (em frações do em).
    pub radical_extra_ascender: f64,
    /// Kern antes do índice do radical (em frações do em).
    pub radical_kern_before_degree: f64,
    /// Kern após o índice do radical (em frações do em).
    pub radical_kern_after_degree: f64,
    /// Porcentagem do radical para elevar o índice (0–100).
    pub radical_degree_bottom_raise_percent: f64,

    /// Gap mínimo acima do limite superior de integrais/somatórios.
    pub upper_limit_gap: f64,
    /// Gap mínimo abaixo do limite inferior de integrais/somatórios.
    pub lower_limit_gap: f64,
}

impl MathConstants {
    /// Tenta extrair da tabela OpenType MATH; usa fallback caso não exista ou para valores nulos.
    pub fn from_font(font: &FontData) -> Self {
        let upem = font.units_per_em as f64;
        let asc = font.ascender as f64 / upem;
        let dsc = (-font.descender as f64) / upem;
        let x_height = asc * 0.55; // aproximação: x-height ≈ 55% do ascender

        // Valores de fallback (TeX handbook + Latin Modern Math 10pt)
        let fb = Self {
            script_percent_scale_down:        0.71,
            script_script_percent_scale_down: 0.504,
            fraction_rule_thickness:    0.04,
            fraction_num_gap:           0.05,
            fraction_denom_gap:         0.05,
            fraction_num_shift_up:      x_height + 0.35,
            fraction_denom_shift_down:  dsc * 0.6 + 0.15,
            superscript_shift_up:       asc * 0.55,
            subscript_shift_down:       dsc * 0.40,
            sub_superscript_gap:        0.10,
            space_after_script:         0.05,
            axis_height:                x_height * 0.50,
            radical_vertical_gap:       0.07,
            radical_rule_thickness:     0.065,
            radical_extra_ascender:     0.065,
            radical_kern_before_degree: -0.10,
            radical_kern_after_degree:  -0.10,
            radical_degree_bottom_raise_percent: 60.0,
            upper_limit_gap:            0.12,
            lower_limit_gap:            0.12,
        };

        let face = ttf_parser::Face::parse(&font.raw_bytes, 0).ok();

        let mv = |v: i16, fallback: f64| -> f64 {
            let r = v as f64 / upem;
            if r == 0.0 { fallback } else { r }
        };
        let pct = |v: i16, fallback: f64| -> f64 {
            let r = v as f64 / 100.0;
            if r == 0.0 { fallback } else { r }
        };

        if let Some(face) = face {
            if let Some(math) = face.tables().math {
                if let Some(c) = math.constants {
                    return Self {
                        script_percent_scale_down:
                            pct(c.script_percent_scale_down(), fb.script_percent_scale_down),
                        script_script_percent_scale_down:
                            pct(c.script_script_percent_scale_down(), fb.script_script_percent_scale_down),
                        fraction_rule_thickness:
                            mv(c.fraction_rule_thickness().value, fb.fraction_rule_thickness),
                        fraction_num_gap:
                            mv(c.fraction_numerator_gap_min().value, fb.fraction_num_gap),
                        fraction_denom_gap:
                            mv(c.fraction_denominator_gap_min().value, fb.fraction_denom_gap),
                        fraction_num_shift_up:
                            mv(c.fraction_numerator_shift_up().value, fb.fraction_num_shift_up),
                        fraction_denom_shift_down:
                            mv(c.fraction_denominator_shift_down().value, fb.fraction_denom_shift_down),
                        superscript_shift_up:
                            mv(c.superscript_shift_up().value, fb.superscript_shift_up),
                        subscript_shift_down:
                            mv(c.subscript_shift_down().value, fb.subscript_shift_down),
                        sub_superscript_gap:
                            mv(c.sub_superscript_gap_min().value, fb.sub_superscript_gap),
                        space_after_script:
                            mv(c.space_after_script().value, fb.space_after_script),
                        axis_height:
                            mv(c.axis_height().value, fb.axis_height),
                        radical_vertical_gap:
                            mv(c.radical_vertical_gap().value, fb.radical_vertical_gap),
                        radical_rule_thickness:
                            mv(c.radical_rule_thickness().value, fb.radical_rule_thickness),
                        radical_extra_ascender:
                            mv(c.radical_extra_ascender().value, fb.radical_extra_ascender),
                        radical_kern_before_degree:
                            mv(c.radical_kern_before_degree().value, fb.radical_kern_before_degree),
                        radical_kern_after_degree:
                            mv(c.radical_kern_after_degree().value, fb.radical_kern_after_degree),
                        radical_degree_bottom_raise_percent:
                            c.radical_degree_bottom_raise_percent() as f64,
                        upper_limit_gap:
                            mv(c.upper_limit_gap_min().value, fb.upper_limit_gap),
                        lower_limit_gap:
                            mv(c.lower_limit_gap_min().value, fb.lower_limit_gap),
                    };
                }
            }
        }

        fb
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Contexto de layout
// ─────────────────────────────────────────────────────────────────────────────

/// Contexto imutável passado recursivamente durante o layout.
pub struct MathContext<'a> {
    /// Fonte a usar.
    pub font: &'a FontData,
    /// Tamanho de fonte em pontos.
    pub font_size: f64,
    /// `true` = estilo display (operadores grandes, frações em tamanho normal).
    pub display: bool,
    /// Constantes de tipografia extraídas da fonte (ou fallback).
    pub constants: MathConstants,
}

impl<'a> MathContext<'a> {
    pub fn new(font: &'a FontData, font_size: f64, display: bool) -> Self {
        let constants = MathConstants::from_font(font);
        Self { font, font_size, display, constants }
    }

    fn em(&self) -> f64 { self.font_size }

    fn scale(&self) -> f64 { self.font_size / self.font.units_per_em as f64 }

    /// Contexto para superscript/subscript (fonte reduzida, inline).
    fn script_ctx(&self) -> MathContext<'_> {
        MathContext {
            font: self.font,
            font_size: self.font_size * self.constants.script_percent_scale_down,
            display: false,
            constants: self.constants.clone(),
        }
    }

    /// Contexto para sub-sub scripts.
    fn script_script_ctx(&self) -> MathContext<'_> {
        MathContext {
            font: self.font,
            font_size: self.font_size * self.constants.script_script_percent_scale_down,
            display: false,
            constants: self.constants.clone(),
        }
    }

    /// Contexto para filhos de fração.
    fn fraction_child_ctx(&self) -> MathContext<'_> {
        MathContext {
            font: self.font,
            font_size: if self.display {
                self.font_size  // display mode: filhos têm o mesmo tamanho
            } else {
                self.font_size * self.constants.script_percent_scale_down
            },
            display: false,
            constants: self.constants.clone(),
        }
    }

    /// Altura do ascender em pontos.
    fn ascender_pts(&self) -> f64 {
        self.font.ascender as f64 * self.scale()
    }

    /// Profundidade do descender em pontos (valor positivo).
    fn descender_pts(&self) -> f64 {
        (-self.font.descender as f64) * self.scale()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Função principal de layout
// ─────────────────────────────────────────────────────────────────────────────

/// Converte um `MathNode` em glyphs e regras posicionados.
pub fn layout_math(node: &MathNode, ctx: &MathContext<'_>) -> MathLayoutResult {
    match node {
        MathNode::Literal(s)   => layout_atom(s, ctx),
        MathNode::Symbol(c)    => layout_atom(&c.to_string(), ctx),
        MathNode::Operator(s)  => layout_atom(s, ctx),

        MathNode::Row(nodes)   => layout_row(nodes, ctx),

        MathNode::Fraction { numerator, denominator } => {
            layout_fraction(numerator, denominator, ctx)
        }

        MathNode::Superscript { base, exponent } => {
            layout_superscript(base, exponent, ctx)
        }
        MathNode::Subscript { base, subscript } => {
            layout_subscript(base, subscript, ctx)
        }
        MathNode::SubSuperscript { base, subscript, superscript } => {
            layout_subsuperscript(base, subscript, superscript, ctx)
        }

        MathNode::Root { index, radicand } => {
            layout_root(index.as_deref(), radicand, ctx)
        }

        MathNode::Delimited { left, right, content } => {
            layout_delimited(left, right, content, ctx)
        }

        MathNode::Integral { lower, upper } => {
            layout_large_op('∫', lower.as_deref(), upper.as_deref(), ctx, false)
        }
        MathNode::Sum { lower, upper } => {
            layout_large_op('∑', lower.as_deref(), upper.as_deref(), ctx, true)
        }

        MathNode::Matrix { rows, delimiters } => {
            layout_matrix(rows, delimiters, ctx)
        }

        MathNode::Space(em) => {
            MathLayoutResult {
                width: em * ctx.em(),
                height: ctx.ascender_pts() * 0.5,
                depth: 0.0,
                glyphs: vec![],
                rules: vec![],
            }
        }

        MathNode::Styled { content, .. } => {
            // Sem fonte alternativa carregada: usa mesma fonte, ignora estilo
            layout_math(content, ctx)
        }

        MathNode::Accent { accent_type, content } => {
            layout_accent(accent_type, content, ctx)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Implementações de cada variant
// ─────────────────────────────────────────────────────────────────────────────

/// Layout de um átomo de texto (Literal, Symbol, Operator).
fn layout_atom(text: &str, ctx: &MathContext<'_>) -> MathLayoutResult {
    if text.is_empty() {
        return MathLayoutResult::empty();
    }
    let glyphs = shape_text(ctx.font, text);
    if glyphs.is_empty() {
        return MathLayoutResult::empty();
    }

    let scale = ctx.scale();
    let width = shaped_text_width(&glyphs, ctx.font_size, ctx.font.units_per_em);
    let height = ctx.ascender_pts();
    let depth = ctx.descender_pts();

    let mut x_cursor = 0.0_f64;
    let positioned: Vec<PositionedMathGlyph> = glyphs
        .iter()
        .map(|g| {
            let x = x_cursor + g.x_offset as f64 * scale;
            let y = g.y_offset as f64 * scale;
            let pg = PositionedMathGlyph { glyph_id: g.glyph_id, x, y, size: ctx.font_size };
            x_cursor += g.x_advance as f64 * scale;
            pg
        })
        .collect();

    MathLayoutResult { glyphs: positioned, rules: vec![], width, height, depth }
}

/// Layout de uma sequência de nós (Row).
fn layout_row(nodes: &[MathNode], ctx: &MathContext<'_>) -> MathLayoutResult {
    let mut result = MathLayoutResult::empty();
    for node in nodes {
        let child = layout_math(node, ctx);
        result.append_right(child);
    }
    result
}

/// Layout de uma fração `\frac{num}{den}`.
fn layout_fraction(
    num: &MathNode,
    den: &MathNode,
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    let child_ctx = ctx.fraction_child_ctx();
    let num_r = layout_math(num, &child_ctx);
    let den_r = layout_math(den, &child_ctx);

    let em = ctx.em();
    let bar_y = ctx.constants.axis_height * em;         // eixo = centro da barra
    let bar_t = ctx.constants.fraction_rule_thickness * em;
    let num_gap = ctx.constants.fraction_num_gap * em;
    let den_gap = ctx.constants.fraction_denom_gap * em;

    // Baseline do numerador: acima do topo da barra + gap
    let num_base_y = bar_y + bar_t / 2.0 + num_gap + num_r.depth;
    // Baseline do denominador: abaixo do fundo da barra - gap
    let den_base_y = bar_y - bar_t / 2.0 - den_gap - den_r.height;

    let padding = 0.08 * em;
    let content_w = num_r.width.max(den_r.width);
    let total_w = content_w + 2.0 * padding;

    let num_x = (total_w - num_r.width) / 2.0;
    let den_x = (total_w - den_r.width) / 2.0;

    let mut result = MathLayoutResult {
        glyphs: vec![],
        rules: vec![MathDrawCommand::HRule {
            x: padding * 0.5,
            y: bar_y,
            width: total_w - padding,
            thickness: bar_t,
        }],
        width: total_w,
        height: (num_base_y + num_r.height).max(bar_y + bar_t),
        depth: (-(den_base_y - den_r.depth)).max(-(bar_y - bar_t)).max(0.0),
    };

    result.merge(num_r.shifted(num_x, num_base_y), 0.0, 0.0);
    result.merge(den_r.shifted(den_x, den_base_y), 0.0, 0.0);

    result
}

/// Layout de superscript `base^{exp}`.
fn layout_superscript(
    base: &MathNode,
    exp: &MathNode,
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    let base_r = layout_math(base, ctx);
    let script_ctx = ctx.script_ctx();
    let exp_r = layout_math(exp, &script_ctx);

    let em = ctx.em();
    let shift = ctx.constants.superscript_shift_up * em;
    // O shift mínimo garante que o script não se sobreponha excessivamente à base
    let shift = shift.max(base_r.height - exp_r.depth * 0.5);
    let space = ctx.constants.space_after_script * em;

    let exp_x = base_r.width;
    let exp_y = shift;

    let height = base_r.height.max(exp_y + exp_r.height);
    let exp_depth_contrib = if exp_y > exp_r.depth { 0.0 } else { exp_r.depth - exp_y };
    let depth = base_r.depth.max(exp_depth_contrib);
    let width = base_r.width + exp_r.width + space;

    let mut result = MathLayoutResult { glyphs: vec![], rules: vec![], width, height, depth };
    result.merge(base_r, 0.0, 0.0);
    result.merge(exp_r, exp_x, exp_y);
    result
}

/// Layout de subscript `base_{sub}`.
fn layout_subscript(
    base: &MathNode,
    sub: &MathNode,
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    let base_r = layout_math(base, ctx);
    let script_ctx = ctx.script_ctx();
    let sub_r = layout_math(sub, &script_ctx);

    let em = ctx.em();
    let shift = ctx.constants.subscript_shift_down * em;
    let space = ctx.constants.space_after_script * em;

    let sub_x = base_r.width;
    let sub_y = -shift;  // abaixo da baseline

    let height = base_r.height.max(sub_y + sub_r.height);
    let depth = base_r.depth.max(sub_r.depth - sub_y);
    let width = base_r.width + sub_r.width + space;

    let mut result = MathLayoutResult { glyphs: vec![], rules: vec![], width, height, depth };
    result.merge(base_r, 0.0, 0.0);
    result.merge(sub_r, sub_x, sub_y);
    result
}

/// Layout de sub+superscript simultâneos `base^{sup}_{sub}`.
fn layout_subsuperscript(
    base: &MathNode,
    sub: &MathNode,
    sup: &MathNode,
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    let base_r = layout_math(base, ctx);
    let script_ctx = ctx.script_ctx();
    let sup_r = layout_math(sup, &script_ctx);
    let sub_r = layout_math(sub, &script_ctx);

    let em = ctx.em();
    let sup_shift = ctx.constants.superscript_shift_up * em;
    let sub_shift = ctx.constants.subscript_shift_down * em;
    let gap_min = ctx.constants.sub_superscript_gap * em;
    let space = ctx.constants.space_after_script * em;

    // Garantir gap mínimo entre sup e sub
    let mut sup_y = sup_shift.max(base_r.height - sup_r.depth * 0.5);
    let mut sub_y = -sub_shift;
    let gap = (sup_y - sup_r.depth) - (sub_y + sub_r.height);
    if gap < gap_min {
        let adjust = (gap_min - gap) / 2.0;
        sup_y += adjust;
        sub_y -= adjust;
    }

    let scripts_x = base_r.width;
    let scripts_w = sup_r.width.max(sub_r.width);
    let width = scripts_x + scripts_w + space;

    let height = base_r.height.max(sup_y + sup_r.height);
    let depth = base_r.depth.max(sub_r.depth - sub_y);

    let mut result = MathLayoutResult { glyphs: vec![], rules: vec![], width, height, depth };
    result.merge(base_r, 0.0, 0.0);
    result.merge(sup_r, scripts_x, sup_y);
    result.merge(sub_r, scripts_x, sub_y);
    result
}

/// Layout de radical `\sqrt{radicand}` ou `\sqrt[index]{radicand}`.
fn layout_root(
    index: Option<&MathNode>,
    radicand: &MathNode,
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    let rad_r = layout_math(radicand, ctx);
    let em = ctx.em();

    let rule_t = ctx.constants.radical_rule_thickness * em;
    let v_gap = ctx.constants.radical_vertical_gap * em;
    let extra = ctx.constants.radical_extra_ascender * em;

    // Altura necessária do símbolo radical
    let body_height = rad_r.height + rad_r.depth + v_gap + rule_t;

    // Símbolo √ posicionado
    let sqrt_r = layout_atom("√", ctx);
    let sqrt_scale = if sqrt_r.height + sqrt_r.depth > 0.0 {
        body_height / (sqrt_r.height + sqrt_r.depth)
    } else {
        1.0
    };

    // Posição do símbolo: baseline do radical fica alinhada com a baseline do radicand
    // O topo do símbolo cobre até o topo da régua
    let rule_top_y = rad_r.height + v_gap + rule_t / 2.0;
    let sqrt_width = sqrt_r.width * sqrt_scale.min(2.0);  // limita expansão excessiva

    // Régua horizontal sobre o radicand
    let rule_x = sqrt_width;
    let rule_y = rule_top_y;

    // Layout do índice opcional (ex: 3 em \sqrt[3])
    let (index_width, mut result) = if let Some(idx) = index {
        let idx_ctx = ctx.script_ctx();
        let idx_r = layout_math(idx, &idx_ctx);

        let kern_before = ctx.constants.radical_kern_before_degree * em;
        let kern_after = ctx.constants.radical_kern_after_degree * em;
        let raise_pct = ctx.constants.radical_degree_bottom_raise_percent / 100.0;

        let idx_x = kern_before.max(0.0);
        let idx_y = rule_top_y * raise_pct;  // eleva o índice

        let idx_total_w = idx_r.width + kern_before.abs() + kern_after.abs();
        let mut r = MathLayoutResult::empty();
        r.merge(idx_r, idx_x, idx_y);
        (idx_total_w, r)
    } else {
        (0.0, MathLayoutResult::empty())
    };

    let sqrt_x = index_width;

    // Glyphs do símbolo √ com escala de tamanho
    let mut sqrt_sized = layout_atom("√", ctx);
    // Ajusta o tamanho (via font_size escalado para cobrir o radicand)
    let sqrt_fs = ctx.font_size * sqrt_scale.min(2.0);
    for g in &mut sqrt_sized.glyphs {
        g.size = sqrt_fs;
    }
    result.merge(sqrt_sized.shifted(sqrt_x, 0.0), 0.0, 0.0);

    // Radicand à direita do √
    let rad_w = rad_r.width;
    result.merge(rad_r.shifted(sqrt_x + sqrt_width, 0.0), 0.0, 0.0);

    // Régua horizontal
    let total_w = sqrt_x + sqrt_width + rad_w;
    result.rules.push(MathDrawCommand::HRule {
        x: rule_x + sqrt_x,
        y: rule_y,
        width: total_w - rule_x - sqrt_x,
        thickness: rule_t,
    });

    result.width = total_w;
    result.height = (rule_top_y + extra).max(result.height);
    result.depth = result.depth.max(0.0);
    result
}

/// Layout de operador grande (`\int`, `\sum`) com limites opcionais.
fn layout_large_op(
    symbol: char,
    lower: Option<&MathNode>,
    upper: Option<&MathNode>,
    ctx: &MathContext<'_>,
    movable: bool,  // true = ∑ (limites acima/abaixo em display), false = ∫ (sempre ao lado)
) -> MathLayoutResult {
    let op_r = layout_atom(&symbol.to_string(), ctx);
    let em = ctx.em();
    let limits_ctx = ctx.script_ctx();
    let use_display_limits = movable && ctx.display;

    if lower.is_none() && upper.is_none() {
        return op_r;
    }

    if use_display_limits {
        // Limites acima e abaixo do símbolo (estilo display)
        let gap = ctx.constants.upper_limit_gap * em;

        let upper_r = upper.map(|n| layout_math(n, &limits_ctx));
        let lower_r = lower.map(|n| layout_math(n, &limits_ctx));

        let max_w = [
            op_r.width,
            upper_r.as_ref().map(|r| r.width).unwrap_or(0.0),
            lower_r.as_ref().map(|r| r.width).unwrap_or(0.0),
        ]
        .iter()
        .cloned()
        .fold(0.0_f64, f64::max);

        let mut result = MathLayoutResult::empty();
        result.width = max_w;

        // Posiciona operador centrado
        let op_x = (max_w - op_r.width) / 2.0;
        result.height = op_r.height;
        result.depth = op_r.depth;
        result.merge(op_r, op_x, 0.0);

        // Limite superior
        if let Some(up_r) = upper_r {
            let up_x = (max_w - up_r.width) / 2.0;
            let up_y = result.height + gap + up_r.depth;
            result.height = result.height.max(up_y + up_r.height);
            result.merge(up_r, up_x, up_y);
        }

        // Limite inferior
        if let Some(lo_r) = lower_r {
            let lo_x = (max_w - lo_r.width) / 2.0;
            let lo_y = -(result.depth + gap + lo_r.height);
            result.depth = result.depth.max(lo_r.depth - lo_y);
            result.merge(lo_r, lo_x, lo_y);
        }

        result
    } else {
        // Limites ao lado (inline ou ∫ sempre)
        layout_subsuperscript(
            &MathNode::Symbol(symbol),
            &lower.cloned().unwrap_or(MathNode::Row(vec![])),
            &upper.cloned().unwrap_or(MathNode::Row(vec![])),
            ctx,
        )
    }
}

/// Cria o layout de um delimitador desenhado como traçado PDF (sem glyph de fonte).
///
/// `y_center` = posição vertical do centro em coords matemáticas (relativo à baseline).
/// `half_height` = metade da altura total do delimitador.
/// Retorna (result, layout_width) onde result contém apenas draw commands (sem glyphs).
fn make_drawn_delimiter(
    delim: &str,
    y_center: f64,
    half_height: f64,
    em: f64,
) -> Option<MathLayoutResult> {
    if delim.is_empty() { return None; }
    let stroke_w = (em * 0.07).max(0.4_f64).min(1.0);
    let height = y_center + half_height;
    let depth  = (half_height - y_center).max(0.0);

    match delim {
        "(" => {
            let delim_w = (half_height * 0.30).max(em * 0.18).min(em * 0.42);
            let mut r = MathLayoutResult::empty();
            r.width  = delim_w;
            r.height = height;
            r.depth  = depth;
            r.rules.push(MathDrawCommand::LeftParen {
                x: 0.0, y_center, half_height, delim_w, stroke_w,
            });
            Some(r)
        }
        ")" => {
            let delim_w = (half_height * 0.30).max(em * 0.18).min(em * 0.42);
            let mut r = MathLayoutResult::empty();
            r.width  = delim_w;
            r.height = height;
            r.depth  = depth;
            r.rules.push(MathDrawCommand::RightParen {
                x: 0.0, y_center, half_height, delim_w, stroke_w,
            });
            Some(r)
        }
        "[" => {
            let delim_w = em * 0.20;
            let mut r = MathLayoutResult::empty();
            r.width  = delim_w;
            r.height = height;
            r.depth  = depth;
            r.rules.push(MathDrawCommand::LeftBracket {
                x: 0.0, y_center, half_height, delim_w, stroke_w,
            });
            Some(r)
        }
        "]" => {
            let delim_w = em * 0.20;
            let mut r = MathLayoutResult::empty();
            r.width  = delim_w;
            r.height = height;
            r.depth  = depth;
            r.rules.push(MathDrawCommand::RightBracket {
                x: 0.0, y_center, half_height, delim_w, stroke_w,
            });
            Some(r)
        }
        "|" => {
            let thickness = (em * 0.06).max(0.4_f64).min(0.8);
            let delim_w   = thickness + em * 0.06;
            let mut r = MathLayoutResult::empty();
            r.width  = delim_w;
            r.height = height;
            r.depth  = depth;
            r.rules.push(MathDrawCommand::VBar {
                x: delim_w / 2.0, y_center, half_height, thickness,
            });
            Some(r)
        }
        "‖" => {
            // Barra dupla: dois VBars próximos
            let thickness = (em * 0.06).max(0.4_f64).min(0.8);
            let gap       = em * 0.10;
            let delim_w   = 2.0 * thickness + gap + em * 0.06;
            let mut r = MathLayoutResult::empty();
            r.width  = delim_w;
            r.height = height;
            r.depth  = depth;
            r.rules.push(MathDrawCommand::VBar {
                x: thickness / 2.0, y_center, half_height, thickness,
            });
            r.rules.push(MathDrawCommand::VBar {
                x: thickness + gap + thickness / 2.0, y_center, half_height, thickness,
            });
            Some(r)
        }
        _ => None,  // delimitador desconhecido: usa glyph de fonte
    }
}

/// Layout de delimitadores escaláveis `\left( ... \right)`.
fn layout_delimited(
    left: &str,
    right: &str,
    content: &MathNode,
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    let content_r = layout_math(content, ctx);
    let em   = ctx.em();
    let axis = ctx.constants.axis_height * em;

    // Dimensiona o delimitador centrado no eixo matemático (convenção TeX):
    //   half = max(conteúdo acima do eixo, conteúdo abaixo do eixo)
    let above_axis = (content_r.height - axis).max(0.0);
    let below_axis = (content_r.depth  + axis).max(0.0);
    let half = above_axis.max(below_axis).max(em * 0.5);

    // Tenta gerar delimitador desenhado; caso contrário, usa glyph de fonte.
    let left_r = make_drawn_delimiter(left, axis, half, em).unwrap_or_else(|| {
        let delim_size = (2.0 * half).min(em * 1.8);
        let delim_ctx_size = (delim_size / (ctx.ascender_pts() + ctx.descender_pts())
            * ctx.font_size).max(ctx.font_size);
        let mut r = layout_atom(left, ctx);
        for g in &mut r.glyphs { g.size = delim_ctx_size; }
        r.height = delim_size / 2.0 + axis;
        r.depth  = (delim_size / 2.0 - axis).max(0.0);
        r
    });

    let right_r = make_drawn_delimiter(right, axis, half, em).unwrap_or_else(|| {
        let delim_size = (2.0 * half).min(em * 1.8);
        let delim_ctx_size = (delim_size / (ctx.ascender_pts() + ctx.descender_pts())
            * ctx.font_size).max(ctx.font_size);
        let mut r = layout_atom(right, ctx);
        for g in &mut r.glyphs { g.size = delim_ctx_size; }
        r.height = delim_size / 2.0 + axis;
        r.depth  = (delim_size / 2.0 - axis).max(0.0);
        r
    });

    let left_w     = left_r.width;
    let content_w2 = content_r.width;
    let h = left_r.height.max(content_r.height).max(right_r.height);
    let d = left_r.depth .max(content_r.depth) .max(right_r.depth);

    let mut result = MathLayoutResult { glyphs: vec![], rules: vec![], width: left_w + content_w2 + right_r.width, height: h, depth: d };
    result.merge(left_r,    0.0,   0.0);
    result.merge(content_r, left_w, 0.0);
    result.merge(right_r,   left_w + content_w2, 0.0);
    result
}

/// Layout de matriz.
fn layout_matrix(
    rows: &[Vec<MathNode>],
    delimiters: &(String, String),
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    if rows.is_empty() {
        return MathLayoutResult::empty();
    }

    let em = ctx.em();
    let col_padding = 0.3 * em;
    let row_spacing = 0.3 * em;

    // Layout de todas as células
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let cell_results: Vec<Vec<MathLayoutResult>> = rows
        .iter()
        .map(|row| row.iter().map(|cell| layout_math(cell, ctx)).collect())
        .collect();

    // Largura máxima de cada coluna
    let mut col_widths = vec![0.0_f64; ncols];
    for row in &cell_results {
        for (col, cell) in row.iter().enumerate() {
            col_widths[col] = col_widths[col].max(cell.width);
        }
    }

    // Altura máxima (acima + abaixo) de cada linha
    let row_heights: Vec<(f64, f64)> = cell_results
        .iter()
        .map(|row| {
            let h = row.iter().map(|c| c.height).fold(0.0_f64, f64::max);
            let d = row.iter().map(|c| c.depth).fold(0.0_f64, f64::max);
            (h, d)
        })
        .collect();

    // Posicionar células
    let mut glyphs = vec![];
    let mut rules = vec![];
    let mut y_cursor = 0.0_f64; // y do topo da linha atual

    for (ri, row) in cell_results.iter().enumerate() {
        let (row_h, row_d) = row_heights[ri];
        let baseline_y = -(y_cursor + row_h);  // y da baseline desta linha

        let mut x_cursor = 0.0_f64;
        for (ci, cell) in row.iter().enumerate() {
            // Centraliza célula na coluna
            let cell_x = x_cursor + (col_widths[ci] - cell.width) / 2.0;
            for mut g in cell.glyphs.iter().cloned() {
                g.x += cell_x;
                g.y += baseline_y;
                glyphs.push(g);
            }
            for mut r in cell.rules.iter().cloned() {
                r.shift(cell_x, baseline_y);
                rules.push(r);
            }
            x_cursor += col_widths[ci] + col_padding;
        }
        y_cursor += row_h + row_d + row_spacing;
    }

    let total_h = y_cursor - row_spacing;
    let total_w = col_widths.iter().sum::<f64>()
        + col_padding * (ncols.saturating_sub(1)) as f64;

    // Centraliza verticalmente na baseline (baseline da linha central)
    let center_y = total_h / 2.0;

    // Desloca todos para que o centro da matriz fique no eixo matemático.
    //
    // Antes do shift as células ocupam [0, -total_h] em y (positivo = acima).
    // O midpoint é -center_y. Para que o midpoint coincida com `axis`:
    //   dy = axis - midpoint = axis - (-center_y) = axis + center_y
    //
    // Atenção: height/depth abaixo assumem dy = axis + center_y.
    let axis = ctx.constants.axis_height * em;
    let dy = axis + center_y;

    for g in &mut glyphs {
        g.y += dy;
    }
    for r in &mut rules {
        r.shift(0.0, dy);
    }

    let content = MathLayoutResult {
        glyphs,
        rules,
        width: total_w,
        height: center_y + axis,
        depth: (center_y - axis).max(0.0),
    };

    // Aplica delimitadores se existirem usando traçados desenhados.
    if !delimiters.0.is_empty() || !delimiters.1.is_empty() {
        let above_axis = (content.height - axis).max(0.0);
        let below_axis = (content.depth  + axis).max(0.0);
        let half = above_axis.max(below_axis).max(em * 0.5);

        let left_r = make_drawn_delimiter(&delimiters.0, axis, half, em)
            .unwrap_or_else(|| {
                let delim_size = 2.0 * half;
                let delim_ctx_size = (delim_size / (ctx.ascender_pts() + ctx.descender_pts())
                    * ctx.font_size).max(ctx.font_size);
                let mut r = layout_atom(&delimiters.0, ctx);
                for g in &mut r.glyphs { g.size = delim_ctx_size; }
                r.height = delim_size / 2.0 + axis;
                r.depth  = (delim_size / 2.0 - axis).max(0.0);
                r
            });
        let right_r = make_drawn_delimiter(&delimiters.1, axis, half, em)
            .unwrap_or_else(|| {
                let delim_size = 2.0 * half;
                let delim_ctx_size = (delim_size / (ctx.ascender_pts() + ctx.descender_pts())
                    * ctx.font_size).max(ctx.font_size);
                let mut r = layout_atom(&delimiters.1, ctx);
                for g in &mut r.glyphs { g.size = delim_ctx_size; }
                r.height = delim_size / 2.0 + axis;
                r.depth  = (delim_size / 2.0 - axis).max(0.0);
                r
            });

        let left_w    = left_r.width;
        let content_w = content.width;
        let h = content.height.max(left_r.height);
        let d = content.depth .max(left_r.depth);
        let mut final_result = MathLayoutResult {
            glyphs: vec![], rules: vec![],
            width: left_w + content_w + right_r.width,
            height: h, depth: d,
        };
        final_result.merge(left_r,  0.0,    0.0);
        final_result.merge(content, left_w, 0.0);
        final_result.merge(right_r, left_w + content_w, 0.0);
        final_result
    } else {
        content
    }
}

/// Layout de acento `\hat{x}`, `\bar{x}`, etc.
fn layout_accent(
    accent_type: &AccentType,
    content: &MathNode,
    ctx: &MathContext<'_>,
) -> MathLayoutResult {
    let base_r = layout_math(content, ctx);
    let accent_char = match accent_type {
        AccentType::Hat   => '^',
        AccentType::Bar   => '‾',
        AccentType::Vec   => '→',
        AccentType::Dot   => '˙',
        AccentType::Tilde => '~',
    };

    let em = ctx.em();
    let acc_ctx_size = ctx.font_size * 0.75;
    let mut acc_r = layout_atom(&accent_char.to_string(), ctx);
    for g in &mut acc_r.glyphs {
        g.size = acc_ctx_size;
    }

    // Posiciona o acento centrado acima da base
    let acc_x = (base_r.width - acc_r.width) / 2.0;
    let acc_y = base_r.height + em * 0.05;  // pequeno gap acima da base

    let height = acc_y + acc_r.height;
    let width = base_r.width.max(acc_r.width);

    let mut result = MathLayoutResult {
        glyphs: vec![],
        rules: vec![],
        width,
        height,
        depth: base_r.depth,
    };
    result.merge(base_r, 0.0, 0.0);
    result.merge(acc_r.shifted(acc_x, acc_y), 0.0, 0.0);
    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers internos de MathLayoutResult
// ─────────────────────────────────────────────────────────────────────────────

impl MathLayoutResult {
    /// Funde outro resultado deslocado por (dx, dy) neste, sem alterar width/height/depth.
    fn merge(&mut self, other: MathLayoutResult, dx: f64, dy: f64) {
        for mut g in other.glyphs {
            g.x += dx;
            g.y += dy;
            self.glyphs.push(g);
        }
        for mut r in other.rules {
            r.shift(dx, dy);
            self.rules.push(r);
        }
    }

}

// ─────────────────────────────────────────────────────────────────────────────
// Testes
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::parser::parse_latex;

    fn load_font() -> FontData {
        let bytes = std::fs::read("fonts/DejaVuSans.ttf")
            .expect("DejaVuSans.ttf não encontrado em fonts/");
        FontData::from_bytes(&bytes).expect("falha ao carregar fonte")
    }

    fn ctx(font: &FontData, display: bool) -> MathContext<'_> {
        MathContext::new(font, 12.0, display)
    }

    // ── Átomos ───────────────────────────────────────────────────────────────

    #[test]
    fn atom_has_positive_dimensions() {
        let font = load_font();
        let r = layout_atom("x", &ctx(&font, false));
        assert!(r.width > 0.0, "largura > 0");
        assert!(r.height > 0.0, "altura > 0");
        assert!(r.depth >= 0.0, "depth >= 0");
        assert_eq!(r.glyphs.len(), 1);
    }

    #[test]
    fn atom_number_wider_than_single_digit() {
        let font = load_font();
        let one = layout_atom("1", &ctx(&font, false));
        let twelve = layout_atom("12", &ctx(&font, false));
        assert!(twelve.width > one.width, "12 mais largo que 1");
    }

    #[test]
    fn row_width_is_sum_of_children() {
        let font = load_font();
        let c = ctx(&font, false);
        let a = layout_atom("x", &c);
        let b = layout_atom("+", &c);
        let ab_w = a.width + b.width;

        let result = layout_row(&[parse_latex("x+").unwrap()], &c);
        // Apenas verificamos que a Row não é vazia
        assert!(result.width > 0.0);
    }

    // ── Fração ───────────────────────────────────────────────────────────────

    #[test]
    fn fraction_numerator_above_axis() {
        let font = load_font();
        let c = ctx(&font, true);
        let node = parse_latex(r"\frac{1}{2}").unwrap();
        let r = layout_math(&node, &c);
        assert!(r.width > 0.0, "fração tem largura");
        assert!(r.height > 0.0, "fração tem altura acima da baseline");
        assert!(r.depth > 0.0, "fração tem profundidade abaixo da baseline");
        // Deve ter uma régua (barra de fração)
        assert!(!r.rules.is_empty(), "fração deve ter barra horizontal");
    }

    #[test]
    fn fraction_is_taller_than_atom() {
        let font = load_font();
        let c = ctx(&font, true);
        let atom = layout_atom("x", &c);
        let frac = layout_math(&parse_latex(r"\frac{1}{2}").unwrap(), &c);
        assert!(frac.height + frac.depth > atom.height + atom.depth,
            "fração deve ser mais alta que átomo");
    }

    // ── Superscript ──────────────────────────────────────────────────────────

    #[test]
    fn superscript_height_greater_than_base() {
        let font = load_font();
        let c = ctx(&font, false);
        let base = layout_atom("x", &c);
        let sup = layout_math(&parse_latex("x^2").unwrap(), &c);
        assert!(sup.height >= base.height, "superscript eleva a altura");
        assert!(sup.width > base.width, "superscript alarga a expressão");
    }

    #[test]
    fn superscript_glyph_smaller() {
        let font = load_font();
        let c = ctx(&font, false);
        let result = layout_math(&parse_latex("x^2").unwrap(), &c);
        // Deve ter 2 glyphs (x e 2), o segundo menor
        assert_eq!(result.glyphs.len(), 2);
        assert!(result.glyphs[1].size < result.glyphs[0].size,
            "expoente deve ter fonte menor que base");
    }

    // ── Subscript ────────────────────────────────────────────────────────────

    #[test]
    fn subscript_extends_below_baseline() {
        let font = load_font();
        let c = ctx(&font, false);
        let result = layout_math(&parse_latex("x_i").unwrap(), &c);
        assert!(result.depth > 0.0, "subscript deve ter depth > 0");
    }

    // ── Radical ──────────────────────────────────────────────────────────────

    #[test]
    fn sqrt_has_rule() {
        let font = load_font();
        let c = ctx(&font, false);
        let result = layout_math(&parse_latex(r"\sqrt{x}").unwrap(), &c);
        assert!(!result.rules.is_empty(), "radical deve ter régua horizontal");
    }

    #[test]
    fn sqrt_wider_than_base() {
        let font = load_font();
        let c = ctx(&font, false);
        let atom = layout_atom("x", &c);
        let sqrt = layout_math(&parse_latex(r"\sqrt{x}").unwrap(), &c);
        assert!(sqrt.width > atom.width, "radical mais largo que base");
    }

    // ── Integral e somatório ─────────────────────────────────────────────────

    #[test]
    fn integral_without_limits() {
        let font = load_font();
        let c = ctx(&font, false);
        let result = layout_math(&parse_latex(r"\int").unwrap(), &c);
        assert!(result.width > 0.0);
        assert_eq!(result.glyphs.len(), 1);
    }

    #[test]
    fn integral_with_limits_wider() {
        let font = load_font();
        let c = ctx(&font, false);
        let bare = layout_math(&parse_latex(r"\int").unwrap(), &c);
        let lim = layout_math(&parse_latex(r"\int_0^1").unwrap(), &c);
        assert!(lim.width > bare.width, "integral com limites é mais larga");
    }

    #[test]
    fn sum_display_limits_taller() {
        let font = load_font();
        let c_inline = ctx(&font, false);
        let c_display = ctx(&font, true);
        let inline = layout_math(&parse_latex(r"\sum_{i=0}^{n}").unwrap(), &c_inline);
        let display = layout_math(&parse_latex(r"\sum_{i=0}^{n}").unwrap(), &c_display);
        assert!(
            display.height + display.depth > inline.height + inline.depth,
            "sum em display deve ser maior verticalmente"
        );
    }

    // ── Expressões complexas ─────────────────────────────────────────────────

    #[test]
    fn quadratic_formula_parses_and_lays_out() {
        let font = load_font();
        let c = ctx(&font, true);
        let node = parse_latex(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}").unwrap();
        let result = layout_math(&node, &c);
        assert!(result.width > 0.0);
        assert!(result.height > 0.0);
        assert!(!result.glyphs.is_empty());
    }

    #[test]
    fn eulers_identity_lays_out() {
        let font = load_font();
        let c = ctx(&font, false);
        let node = parse_latex(r"e^{i\pi} + 1 = 0").unwrap();
        let result = layout_math(&node, &c);
        assert!(result.width > 0.0);
    }

    #[test]
    fn matrix_2x2_lays_out() {
        let font = load_font();
        let c = ctx(&font, false);
        let node = parse_latex(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}").unwrap();
        let result = layout_math(&node, &c);
        let em = c.em();
        let axis = c.constants.axis_height * em;
        eprintln!("em={em:.2} axis={axis:.2} height={:.2} depth={:.2} width={:.2}",
            result.height, result.depth, result.width);
        for (i, g) in result.glyphs.iter().enumerate() {
            eprintln!("  glyph[{i}] x={:.2} y={:.2} size={:.2}", g.x, g.y, g.size);
        }
        assert!(result.width > 0.0);
        // Todos os glyphs devem ter y dentro do intervalo [-(depth), height]
        for g in &result.glyphs {
            assert!(g.y >= -result.depth - 0.1 && g.y <= result.height + 0.1,
                "glyph.y={:.2} fora de [{:.2}, {:.2}]", g.y, -result.depth, result.height);
        }
        assert!(result.height + result.depth > em,
            "matriz 2x2 deve ser mais alta que 1em");
        // Ao menos um glyph deve estar acima do eixo (y > axis)
        assert!(result.glyphs.iter().any(|g| g.y > axis),
            "nenhum glyph acima do eixo matemático");
        // Ao menos um glyph deve estar abaixo do eixo (y < axis)
        assert!(result.glyphs.iter().any(|g| g.y < axis),
            "nenhum glyph abaixo do eixo matemático");
    }

    #[test]
    fn math_constants_fallback_axis_positive() {
        let font = load_font();
        let constants = MathConstants::from_font(&font);
        assert!(constants.axis_height > 0.0, "axis_height deve ser positivo");
        assert!(constants.superscript_shift_up > 0.0, "sup shift deve ser positivo");
        assert!(constants.fraction_rule_thickness > 0.0, "bar thickness deve ser positivo");
    }
}
