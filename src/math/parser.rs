//! Parser LaTeX math via `pulldown-latex` 0.7.
//!
//! A função `parse_latex` converte uma string LaTeX em `MathNode`.
//! A função `latex_to_mathml` converte para MathML.

// ─────────────────────────────────────────────────────────────────────────────
// AST pública (API inalterada)
// ─────────────────────────────────────────────────────────────────────────────

/// Nó da AST de uma expressão LaTeX math.
#[derive(Debug, Clone, PartialEq)]
pub enum MathNode {
    /// Número ou texto literal (ex: "42", "3.14").
    Literal(String),
    /// Letra ou símbolo Unicode (ex: 'x', 'α', '∞').
    Symbol(char),
    /// Operador ou função (ex: "+", "≤", "→", "sin").
    Operator(String),
    /// Fração: `\frac{num}{den}`.
    Fraction { numerator: Box<MathNode>, denominator: Box<MathNode> },
    /// Superscript: `base^{exp}`.
    Superscript { base: Box<MathNode>, exponent: Box<MathNode> },
    /// Subscript: `base_{sub}`.
    Subscript { base: Box<MathNode>, subscript: Box<MathNode> },
    /// Super+subscript simultâneos: `base^{exp}_{sub}`.
    SubSuperscript {
        base: Box<MathNode>,
        subscript: Box<MathNode>,
        superscript: Box<MathNode>,
    },
    /// Raiz: `\sqrt{x}` ou `\sqrt[n]{x}`.
    Root { index: Option<Box<MathNode>>, radicand: Box<MathNode> },
    /// Delimitador escalável: `\left( ... \right)`.
    Delimited { left: String, right: String, content: Box<MathNode> },
    /// Sequência de nós.
    Row(Vec<MathNode>),
    /// Integral: `\int_{a}^{b}`.
    Integral { lower: Option<Box<MathNode>>, upper: Option<Box<MathNode>> },
    /// Somatório / Produtório: `\sum_{k=1}^{n}`.
    Sum { lower: Option<Box<MathNode>>, upper: Option<Box<MathNode>> },
    /// Matriz: `\begin{pmatrix}...\end{pmatrix}`.
    Matrix { rows: Vec<Vec<MathNode>>, delimiters: (String, String) },
    /// Espaçamento horizontal em em.
    Space(f64),
    /// Conteúdo com estilo de fonte.
    Styled { style: MathStyle, content: Box<MathNode> },
    /// Acento sobre um símbolo.
    Accent { accent_type: AccentType, content: Box<MathNode> },
}

/// Estilo de fonte matemática.
#[derive(Debug, Clone, PartialEq)]
pub enum MathStyle {
    Text,
    Bold,
    Roman,
    Italic,
    Calligraphic,
}

/// Tipo de acento.
#[derive(Debug, Clone, PartialEq)]
pub enum AccentType {
    Hat,
    Bar,
    Vec,
    Dot,
    Tilde,
}

/// Erro de parsing LaTeX.
#[derive(Debug, Clone, PartialEq)]
pub struct MathError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for MathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "math parse error at {}: {}", self.position, self.message)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// API pública
// ─────────────────────────────────────────────────────────────────────────────

/// Parseia uma expressão LaTeX math e retorna uma `MathNode` AST.
#[cfg(feature = "math")]
pub fn parse_latex(input: &str) -> Result<MathNode, MathError> {
    use pulldown_latex::{Parser as LatexParser, Storage};

    let storage = Storage::new();
    let parser = LatexParser::new(input, &storage);
    let events: Vec<pulldown_latex::event::Event<'_>> = parser
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MathError { message: e.to_string(), position: 1 })?;

    let mut idx = 0;
    let nodes = parse_sequence(&events, &mut idx);
    Ok(normalize(nodes))
}

/// Parseia uma expressão LaTeX math e retorna uma `MathNode` AST.
#[cfg(not(feature = "math"))]
pub fn parse_latex(_input: &str) -> Result<MathNode, MathError> {
    Err(MathError { message: "feature `math` não habilitada".to_string(), position: 0 })
}

/// Converte LaTeX math para uma string MathML usando pulldown-latex.
#[cfg(feature = "math")]
pub fn latex_to_mathml(input: &str) -> Result<String, MathError> {
    use pulldown_latex::{push_mathml, Parser as LatexParser, RenderConfig, Storage};

    let storage = Storage::new();
    let parser = LatexParser::new(input, &storage);
    let mut output = String::new();
    push_mathml(&mut output, parser, RenderConfig::default())
        .map_err(|e| MathError { message: e.to_string(), position: 1 })?;
    Ok(output)
}

/// Converte LaTeX math para uma string MathML usando pulldown-latex.
#[cfg(not(feature = "math"))]
pub fn latex_to_mathml(_input: &str) -> Result<String, MathError> {
    Err(MathError { message: "feature `math` não habilitada".to_string(), position: 0 })
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversão de eventos pulldown-latex → MathNode
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "math")]
use pulldown_latex::event::{
    Content, DimensionUnit, EnvironmentFlow, Event, Font, Grouping, ScriptPosition, ScriptType,
    StateChange, Visual,
};

/// Parseia uma sequência de eventos até atingir fim, End ou fluxo de ambiente.
#[cfg(feature = "math")]
fn parse_sequence(events: &[Event<'_>], idx: &mut usize) -> Vec<MathNode> {
    let mut nodes = Vec::new();
    while *idx < events.len() {
        match &events[*idx] {
            Event::End => break,
            Event::EnvironmentFlow(EnvironmentFlow::Alignment) => break,
            Event::EnvironmentFlow(EnvironmentFlow::NewLine { .. }) => break,
            Event::StateChange(change) => {
                *idx += 1;
                let change = change.clone();
                // StateChange aplica-se ao restante do escopo atual
                let remaining = parse_sequence(events, idx);
                match change {
                    StateChange::Font(Some(font)) => {
                        let style = font_to_style(font);
                        let content = normalize(remaining);
                        nodes.push(MathNode::Styled { style, content: Box::new(content) });
                    }
                    _ => nodes.extend(remaining),
                }
                return nodes;
            }
            _ => {
                if let Some(node) = parse_element(events, idx) {
                    nodes.push(node);
                } else {
                    *idx += 1; // skip unhandled
                }
            }
        }
    }
    nodes
}

/// Parseia um único elemento lógico.
#[cfg(feature = "math")]
fn parse_element(events: &[Event<'_>], idx: &mut usize) -> Option<MathNode> {
    if *idx >= events.len() {
        return None;
    }

    match &events[*idx] {
        // ── Conteúdo folha ───────────────────────────────────────────────────
        Event::Content(c) => {
            let node = content_to_node(c);
            *idx += 1;
            Some(node)
        }

        // ── Grupo Begin/End ──────────────────────────────────────────────────
        Event::Begin(grouping) => {
            *idx += 1;
            let g = grouping.clone();
            parse_group(events, idx, &g)
        }

        // ── Frações e raízes (Visual) ────────────────────────────────────────
        Event::Visual(v) => {
            *idx += 1;
            match v {
                Visual::Fraction(_) => {
                    let num = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    let den = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    Some(MathNode::Fraction {
                        numerator: Box::new(num),
                        denominator: Box::new(den),
                    })
                }
                Visual::SquareRoot => {
                    let radicand = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    Some(MathNode::Root { index: None, radicand: Box::new(radicand) })
                }
                Visual::Root => {
                    // pulldown-latex: radicand primeiro, depois índice
                    let radicand = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    let index = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    Some(MathNode::Root {
                        index: Some(Box::new(index)),
                        radicand: Box::new(radicand),
                    })
                }
                Visual::Negation => {
                    // Ignora negação, retorna próximo elemento
                    parse_element(events, idx)
                }
            }
        }

        // ── Scripts (sub/sup) ────────────────────────────────────────────────
        Event::Script { ty, position } => {
            *idx += 1;
            let ty = *ty;
            let pos = *position;
            let base = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));

            match ty {
                ScriptType::Superscript => {
                    let sup = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    Some(build_superscript(base, sup, pos))
                }
                ScriptType::Subscript => {
                    let sub = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    Some(build_subscript(base, sub))
                }
                ScriptType::SubSuperscript => {
                    let sub = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    let sup = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    Some(build_subsuperscript(base, sub, sup))
                }
            }
        }

        // ── StateChange no nível de elemento ────────────────────────────────
        Event::StateChange(change) => {
            *idx += 1;
            let change = change.clone();
            match change {
                StateChange::Font(Some(font)) => {
                    let style = font_to_style(font);
                    let next = parse_element(events, idx).unwrap_or(MathNode::Row(vec![]));
                    Some(MathNode::Styled { style, content: Box::new(next) })
                }
                _ => parse_element(events, idx),
            }
        }

        // ── Espaçamento ──────────────────────────────────────────────────────
        Event::Space { width, .. } => {
            *idx += 1;
            let em = width
                .as_ref()
                .map(|d| dimension_to_em(d.value, d.unit))
                .unwrap_or(0.0);
            Some(MathNode::Space(em))
        }

        // ── Fluxo de ambiente (não deve aparecer aqui) ───────────────────────
        Event::EnvironmentFlow(_) | Event::End => None,
    }
}

/// Parseia o conteúdo de um grupo após consumir Begin.
#[cfg(feature = "math")]
fn parse_group(events: &[Event<'_>], idx: &mut usize, grouping: &Grouping) -> Option<MathNode> {
    match grouping {
        // ── Matriz e ambientes similares ─────────────────────────────────────
        Grouping::Matrix { .. } => {
            let rows = parse_matrix_rows(events, idx);
            Some(MathNode::Matrix { rows, delimiters: ("".to_string(), "".to_string()) })
        }
        Grouping::Cases { left } => {
            let rows = parse_matrix_rows(events, idx);
            let delimiters = if *left {
                ("{".to_string(), "".to_string())
            } else {
                ("".to_string(), "}".to_string())
            };
            Some(MathNode::Matrix { rows, delimiters })
        }
        Grouping::Align { .. }
        | Grouping::Aligned
        | Grouping::Alignat { .. }
        | Grouping::Alignedat { .. }
        | Grouping::Gather { .. }
        | Grouping::Gathered
        | Grouping::Multline
        | Grouping::Split
        | Grouping::Equation { .. } => {
            let rows = parse_matrix_rows(events, idx);
            let cells: Vec<MathNode> = rows.into_iter().flatten().collect();
            Some(MathNode::Row(cells))
        }
        Grouping::SubArray { .. } | Grouping::Array(_) => {
            let rows = parse_matrix_rows(events, idx);
            Some(MathNode::Matrix { rows, delimiters: ("".to_string(), "".to_string()) })
        }

        // ── \left...\right ───────────────────────────────────────────────────
        Grouping::LeftRight(left, right) => {
            let l = left.map(|c| c.to_string()).unwrap_or_default();
            let r = right.map(|c| c.to_string()).unwrap_or_default();
            let mut children = parse_group_children(events, idx);

            // Se único filho for Matrix, absorve delimitadores nela
            if children.len() == 1 {
                if let MathNode::Matrix { .. } = &children[0] {
                    if let MathNode::Matrix { rows, .. } = children.remove(0) {
                        return Some(MathNode::Matrix { rows, delimiters: (l, r) });
                    }
                }
            }

            let content = normalize(children);
            Some(MathNode::Delimited { left: l, right: r, content: Box::new(content) })
        }

        // ── Grupo normal {} ──────────────────────────────────────────────────
        Grouping::Normal => {
            let children = parse_group_children(events, idx);
            Some(normalize(children))
        }
    }
}

/// Parseia filhos de um grupo Normal ou LeftRight até o End correspondente.
#[cfg(feature = "math")]
fn parse_group_children(events: &[Event<'_>], idx: &mut usize) -> Vec<MathNode> {
    let mut nodes = Vec::new();

    while *idx < events.len() {
        match &events[*idx] {
            Event::End => {
                *idx += 1;
                break;
            }
            Event::StateChange(change) => {
                *idx += 1;
                let change = change.clone();
                let remaining = parse_group_children_no_end(events, idx);
                match change {
                    StateChange::Font(Some(font)) => {
                        let style = font_to_style(font);
                        let content = normalize(remaining);
                        nodes.push(MathNode::Styled { style, content: Box::new(content) });
                    }
                    _ => nodes.extend(remaining),
                }
                // End já foi consumido por parse_group_children_no_end
                return nodes;
            }
            _ => {
                if let Some(node) = parse_element(events, idx) {
                    nodes.push(node);
                } else {
                    *idx += 1;
                }
            }
        }
    }
    nodes
}

/// Parseia filhos restantes de um grupo após StateChange, consumindo o End final.
#[cfg(feature = "math")]
fn parse_group_children_no_end(events: &[Event<'_>], idx: &mut usize) -> Vec<MathNode> {
    let mut nodes = Vec::new();
    while *idx < events.len() {
        match &events[*idx] {
            Event::End => {
                *idx += 1;
                break;
            }
            _ => {
                if let Some(node) = parse_element(events, idx) {
                    nodes.push(node);
                } else {
                    *idx += 1;
                }
            }
        }
    }
    nodes
}

/// Parseia o conteúdo de uma matriz (linhas separadas por NewLine, células por Alignment).
#[cfg(feature = "math")]
fn parse_matrix_rows(events: &[Event<'_>], idx: &mut usize) -> Vec<Vec<MathNode>> {
    // Pula StartLines se presente
    if *idx < events.len() {
        if let Event::EnvironmentFlow(EnvironmentFlow::StartLines { .. }) = &events[*idx] {
            *idx += 1;
        }
    }

    let mut rows: Vec<Vec<MathNode>> = vec![];
    let mut current_row: Vec<MathNode> = vec![];

    loop {
        // Parseia elementos de uma célula até Alignment, NewLine ou End
        let cell_nodes = parse_matrix_cell(events, idx);
        let cell = normalize(cell_nodes);
        current_row.push(cell);

        if *idx >= events.len() {
            break;
        }

        match &events[*idx] {
            Event::End => {
                *idx += 1;
                break;
            }
            Event::EnvironmentFlow(EnvironmentFlow::Alignment) => {
                *idx += 1; // consume &, continua na mesma linha
            }
            Event::EnvironmentFlow(EnvironmentFlow::NewLine { .. }) => {
                *idx += 1; // consume \\
                rows.push(current_row);
                current_row = vec![];
            }
            _ => break,
        }
    }

    if !current_row.is_empty() {
        rows.push(current_row);
    }
    rows
}

/// Parseia elementos de uma célula de matriz (para no Alignment/NewLine/End).
#[cfg(feature = "math")]
fn parse_matrix_cell(events: &[Event<'_>], idx: &mut usize) -> Vec<MathNode> {
    let mut nodes = Vec::new();
    while *idx < events.len() {
        match &events[*idx] {
            Event::End => break,
            Event::EnvironmentFlow(EnvironmentFlow::Alignment) => break,
            Event::EnvironmentFlow(EnvironmentFlow::NewLine { .. }) => break,
            Event::StateChange(change) => {
                *idx += 1;
                let change = change.clone();
                let remaining = parse_matrix_cell(events, idx);
                match change {
                    StateChange::Font(Some(font)) => {
                        let style = font_to_style(font);
                        let content = normalize(remaining);
                        nodes.push(MathNode::Styled { style, content: Box::new(content) });
                    }
                    _ => nodes.extend(remaining),
                }
                return nodes;
            }
            _ => {
                if let Some(node) = parse_element(events, idx) {
                    nodes.push(node);
                } else {
                    *idx += 1;
                }
            }
        }
    }
    nodes
}

// ─────────────────────────────────────────────────────────────────────────────
// Construtores de nós com detecção de Integral/Sum/Acento
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "math")]
fn build_superscript(base: MathNode, sup: MathNode, pos: ScriptPosition) -> MathNode {
    // Detecta acento (Script AboveBelow com char de acento como superscript)
    if pos == ScriptPosition::AboveBelow {
        if let Some(at) = detect_accent(&sup) {
            return MathNode::Accent { accent_type: at, content: Box::new(base) };
        }
    }
    // Detecta Integral/Sum sem limites recebendo upper
    match base {
        MathNode::Integral { lower, upper: None } => {
            MathNode::Integral { lower, upper: Some(Box::new(sup)) }
        }
        MathNode::Sum { lower, upper: None } => {
            MathNode::Sum { lower, upper: Some(Box::new(sup)) }
        }
        _ => MathNode::Superscript { base: Box::new(base), exponent: Box::new(sup) },
    }
}

#[cfg(feature = "math")]
fn build_subscript(base: MathNode, sub: MathNode) -> MathNode {
    match base {
        MathNode::Integral { lower: None, upper } => {
            MathNode::Integral { lower: Some(Box::new(sub)), upper }
        }
        MathNode::Sum { lower: None, upper } => {
            MathNode::Sum { lower: Some(Box::new(sub)), upper }
        }
        _ => MathNode::Subscript { base: Box::new(base), subscript: Box::new(sub) },
    }
}

#[cfg(feature = "math")]
fn build_subsuperscript(base: MathNode, sub: MathNode, sup: MathNode) -> MathNode {
    match base {
        MathNode::Integral { .. } => MathNode::Integral {
            lower: Some(Box::new(sub)),
            upper: Some(Box::new(sup)),
        },
        MathNode::Sum { .. } => MathNode::Sum {
            lower: Some(Box::new(sub)),
            upper: Some(Box::new(sup)),
        },
        _ => MathNode::SubSuperscript {
            base: Box::new(base),
            subscript: Box::new(sub),
            superscript: Box::new(sup),
        },
    }
}

/// Detecta caractere de acento em um MathNode Symbol.
#[cfg(feature = "math")]
fn detect_accent(node: &MathNode) -> Option<AccentType> {
    if let MathNode::Symbol(c) = node {
        return match c {
            '^' => Some(AccentType::Hat),
            '~' => Some(AccentType::Tilde),
            '→' => Some(AccentType::Vec),
            '‾' => Some(AccentType::Bar),
            '˙' | '¨' => Some(AccentType::Dot),
            _ => None,
        };
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversão de Content → MathNode
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "math")]
fn content_to_node(c: &Content<'_>) -> MathNode {
    match c {
        Content::Number(s) => MathNode::Literal(s.to_string()),
        Content::Text(s) => {
            MathNode::Styled {
                style: MathStyle::Text,
                content: Box::new(MathNode::Literal(s.to_string())),
            }
        }
        Content::Function(f) => MathNode::Operator(f.to_string()),
        Content::Ordinary { content, .. } => MathNode::Symbol(*content),
        Content::Delimiter { content, .. } => MathNode::Symbol(*content),
        Content::Punctuation(c) => MathNode::Symbol(*c),
        Content::BinaryOp { content, .. } => MathNode::Operator(content.to_string()),
        Content::Relation { content, .. } => {
            let mut buf = [0u8; 8];
            let bytes = content.encode_utf8_to_buf(&mut buf);
            MathNode::Operator(String::from_utf8_lossy(bytes).to_string())
        }
        Content::LargeOp { content, .. } => large_op_to_node(*content),
    }
}

#[cfg(feature = "math")]
fn large_op_to_node(c: char) -> MathNode {
    match c {
        '∫' | '∬' | '∭' | '⨌' | '∮' | '∱' | '∲' | '⨙' | '⨚' => {
            MathNode::Integral { lower: None, upper: None }
        }
        '∑' | '∏' | '∐' => MathNode::Sum { lower: None, upper: None },
        _ => MathNode::Operator(c.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilitários
// ─────────────────────────────────────────────────────────────────────────────

/// Normaliza um Vec<MathNode>: vazio → Row([]), único → o próprio, múltiplos → Row.
fn normalize(nodes: Vec<MathNode>) -> MathNode {
    match nodes.len() {
        0 => MathNode::Row(vec![]),
        1 => nodes.into_iter().next().unwrap(),
        _ => MathNode::Row(nodes),
    }
}

#[cfg(feature = "math")]
fn font_to_style(font: Font) -> MathStyle {
    match font {
        Font::Bold | Font::BoldItalic | Font::BoldSansSerif | Font::SansSerifBoldItalic => {
            MathStyle::Bold
        }
        Font::UpRight | Font::Monospace | Font::SansSerif => MathStyle::Roman,
        Font::Italic | Font::SansSerifItalic => MathStyle::Italic,
        Font::Script | Font::BoldScript | Font::DoubleStruck | Font::Fraktur
        | Font::BoldFraktur => MathStyle::Calligraphic,
    }
}

#[cfg(feature = "math")]
fn dimension_to_em(value: f32, unit: DimensionUnit) -> f64 {
    match unit {
        DimensionUnit::Em => value as f64,
        DimensionUnit::Mu => (value as f64) / 18.0,
        DimensionUnit::Ex => (value as f64) * 0.5, // aproximação: 1ex ≈ 0.5em
        DimensionUnit::Pt => (value as f64) / 10.0,
        DimensionUnit::Pc => (value as f64) * 1.2,
        DimensionUnit::In => (value as f64) * 72.0 / 10.0,
        DimensionUnit::Cm => (value as f64) * 28.35 / 10.0,
        DimensionUnit::Mm => (value as f64) * 2.835 / 10.0,
        _ => value as f64,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Testes
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> MathNode {
        parse_latex(s).unwrap_or_else(|e| panic!("parse error em '{s}': {e}"))
    }

    fn sym(c: char) -> MathNode {
        MathNode::Symbol(c)
    }
    fn lit(s: &str) -> MathNode {
        MathNode::Literal(s.to_string())
    }
    fn op(s: &str) -> MathNode {
        MathNode::Operator(s.to_string())
    }
    fn row(v: Vec<MathNode>) -> MathNode {
        MathNode::Row(v)
    }
    fn sup(base: MathNode, exp: MathNode) -> MathNode {
        MathNode::Superscript { base: Box::new(base), exponent: Box::new(exp) }
    }
    fn sub(base: MathNode, s: MathNode) -> MathNode {
        MathNode::Subscript { base: Box::new(base), subscript: Box::new(s) }
    }
    fn subsup(base: MathNode, s: MathNode, e: MathNode) -> MathNode {
        MathNode::SubSuperscript {
            base: Box::new(base),
            subscript: Box::new(s),
            superscript: Box::new(e),
        }
    }
    fn frac(n: MathNode, d: MathNode) -> MathNode {
        MathNode::Fraction { numerator: Box::new(n), denominator: Box::new(d) }
    }
    fn sqrt(radicand: MathNode) -> MathNode {
        MathNode::Root { index: None, radicand: Box::new(radicand) }
    }
    fn root(idx: MathNode, radicand: MathNode) -> MathNode {
        MathNode::Root { index: Some(Box::new(idx)), radicand: Box::new(radicand) }
    }

    // ── Literais e símbolos ───────────────────────────────────────────────────

    #[test]
    fn single_symbol() {
        assert_eq!(parse("x"), sym('x'));
    }

    #[test]
    fn single_number() {
        assert_eq!(parse("42"), lit("42"));
    }

    #[test]
    fn decimal_number() {
        assert_eq!(parse("3.14"), lit("3.14"));
    }

    #[test]
    fn operator_plus() {
        // '+' é BinaryOp em pulldown-latex
        assert_eq!(parse("+"), op("+"));
    }

    #[test]
    fn sequence_x_plus_1() {
        assert_eq!(parse("x+1"), row(vec![sym('x'), op("+"), lit("1")]));
    }

    // ── Superscript e subscript ───────────────────────────────────────────────

    #[test]
    fn superscript_x2() {
        assert_eq!(parse("x^2"), sup(sym('x'), lit("2")));
    }

    #[test]
    fn superscript_group() {
        assert_eq!(parse("x^{n+1}"), sup(sym('x'), row(vec![sym('n'), op("+"), lit("1")])));
    }

    #[test]
    fn subscript_x_i() {
        assert_eq!(parse("x_i"), sub(sym('x'), sym('i')));
    }

    #[test]
    fn subscript_group() {
        assert_eq!(parse("a_{ij}"), sub(sym('a'), row(vec![sym('i'), sym('j')])));
    }

    #[test]
    fn sub_and_superscript() {
        let result = parse("x_i^2");
        assert_eq!(result, subsup(sym('x'), sym('i'), lit("2")));
    }

    #[test]
    fn sup_then_sub() {
        // x^2_i normalizado para SubSuperscript(base, sub, sup)
        let result = parse("x^2_i");
        assert_eq!(result, subsup(sym('x'), sym('i'), lit("2")));
    }

    // ── Fração ────────────────────────────────────────────────────────────────

    #[test]
    fn frac_one_half() {
        assert_eq!(parse(r"\frac{1}{2}"), frac(lit("1"), lit("2")));
    }

    #[test]
    fn frac_nested() {
        let outer = frac(frac(sym('a'), sym('b')), sym('c'));
        assert_eq!(parse(r"\frac{\frac{a}{b}}{c}"), outer);
    }

    #[test]
    fn frac_with_expressions() {
        let num = row(vec![sym('x'), op("+"), lit("1")]);
        // pulldown-latex normaliza '-' (U+002D) para '−' (U+2212 MINUS SIGN)
        let den = row(vec![sym('x'), op("−"), lit("1")]);
        assert_eq!(parse(r"\frac{x+1}{x-1}"), frac(num, den));
    }

    // ── Raiz ──────────────────────────────────────────────────────────────────

    #[test]
    fn sqrt_simple() {
        assert_eq!(parse(r"\sqrt{x}"), sqrt(sym('x')));
    }

    #[test]
    fn sqrt_with_index() {
        let expected = root(lit("3"), row(vec![sym('x'), op("+"), lit("1")]));
        assert_eq!(parse(r"\sqrt[3]{x+1}"), expected);
    }

    #[test]
    fn sqrt_nested() {
        assert_eq!(parse(r"\sqrt{\sqrt{x}}"), sqrt(sqrt(sym('x'))));
    }

    // ── Letras gregas ─────────────────────────────────────────────────────────

    #[test]
    fn greek_alpha() {
        assert_eq!(parse(r"\alpha"), sym('α'));
    }

    #[test]
    fn greek_omega() {
        assert_eq!(parse(r"\omega"), sym('ω'));
    }

    #[test]
    fn greek_pi() {
        assert_eq!(parse(r"\pi"), sym('π'));
    }

    #[test]
    fn greek_uppercase_sigma() {
        assert_eq!(parse(r"\Sigma"), sym('Σ'));
    }

    #[test]
    fn greek_uppercase_delta() {
        assert_eq!(parse(r"\Delta"), sym('Δ'));
    }

    #[test]
    fn greek_lambda() {
        assert_eq!(parse(r"\lambda"), sym('λ'));
    }

    // ── Operadores ────────────────────────────────────────────────────────────

    #[test]
    fn operator_cdot() {
        // pulldown-latex usa U+22C5 (DOT OPERATOR), diferente de U+00B7 (MIDDLE DOT)
        assert_eq!(parse(r"\cdot"), op("⋅"));
    }

    #[test]
    fn operator_times() {
        assert_eq!(parse(r"\times"), op("×"));
    }

    #[test]
    fn operator_leq() {
        assert_eq!(parse(r"\leq"), op("≤"));
    }

    #[test]
    fn operator_geq() {
        assert_eq!(parse(r"\geq"), op("≥"));
    }

    #[test]
    fn operator_neq() {
        assert_eq!(parse(r"\neq"), op("≠"));
    }

    #[test]
    fn operator_rightarrow() {
        assert_eq!(parse(r"\to"), op("→"));
    }

    #[test]
    fn symbol_infty() {
        assert_eq!(parse(r"\infty"), sym('∞'));
    }

    #[test]
    fn symbol_partial() {
        assert_eq!(parse(r"\partial"), sym('∂'));
    }

    // ── Integral e somatório ──────────────────────────────────────────────────

    #[test]
    fn integral_no_limits() {
        assert_eq!(
            parse(r"\int"),
            MathNode::Integral { lower: None, upper: None }
        );
    }

    #[test]
    fn integral_with_limits() {
        let result = parse(r"\int_0^1");
        assert_eq!(
            result,
            MathNode::Integral {
                lower: Some(Box::new(lit("0"))),
                upper: Some(Box::new(lit("1"))),
            }
        );
    }

    #[test]
    fn integral_full_expression() {
        // \int_0^1 x^2 \, dx → Row [Integral{0,1}, Superscript{x,2}, Space, d, x]
        let result = parse(r"\int_0^1 x^2 \, dx");
        match result {
            MathNode::Row(nodes) => {
                assert_eq!(nodes.len(), 5, "deve ter 5 nós: integral, x^2, espaço, d, x");
                assert!(matches!(&nodes[0], MathNode::Integral { .. }));
                assert!(matches!(&nodes[1], MathNode::Superscript { .. }));
                assert!(matches!(&nodes[2], MathNode::Space(_)));
                assert_eq!(nodes[3], sym('d'));
                assert_eq!(nodes[4], sym('x'));
            }
            other => panic!("esperava Row, got {other:?}"),
        }
    }

    #[test]
    fn sum_with_limits() {
        let result = parse(r"\sum_{i=0}^{n}");
        match result {
            MathNode::Sum { lower: Some(l), upper: Some(u) } => {
                assert!(matches!(*l, MathNode::Row(_)));
                assert_eq!(*u, sym('n'));
            }
            other => panic!("esperava Sum, got {other:?}"),
        }
    }

    // ── Delimitadores ─────────────────────────────────────────────────────────

    #[test]
    fn left_right_parens() {
        let result = parse(r"\left( x+1 \right)");
        match result {
            MathNode::Delimited { left, right, .. } => {
                assert_eq!(left, "(");
                assert_eq!(right, ")");
            }
            other => panic!("esperava Delimited, got {other:?}"),
        }
    }

    #[test]
    fn left_right_brackets() {
        let result = parse(r"\left[ a \right]");
        assert!(matches!(result, MathNode::Delimited { ref left, ref right, .. } if left == "[" && right == "]"));
    }

    #[test]
    fn nested_left_right() {
        parse(r"\left( \left[ x \right] \right)");
    }

    // ── Estilos ───────────────────────────────────────────────────────────────

    #[test]
    fn text_style() {
        let result = parse(r"\text{hello world}");
        match result {
            MathNode::Styled { style: MathStyle::Text, content } => {
                assert_eq!(*content, MathNode::Literal("hello world".to_string()));
            }
            other => panic!("esperava Styled(Text), got {other:?}"),
        }
    }

    #[test]
    fn mathbf_style() {
        let result = parse(r"\mathbf{x}");
        assert!(matches!(result, MathNode::Styled { style: MathStyle::Bold, .. }));
    }

    #[test]
    fn mathrm_style() {
        let result = parse(r"\mathrm{d}");
        assert!(matches!(result, MathNode::Styled { style: MathStyle::Roman, .. }));
    }

    // ── Acentos ───────────────────────────────────────────────────────────────

    #[test]
    fn hat_accent() {
        let result = parse(r"\hat{x}");
        assert!(
            matches!(&result, MathNode::Accent { accent_type: AccentType::Hat, .. }),
            "got {result:?}"
        );
    }

    #[test]
    fn bar_accent() {
        let result = parse(r"\bar{x}");
        assert!(matches!(result, MathNode::Accent { accent_type: AccentType::Bar, .. }));
    }

    #[test]
    fn vec_accent() {
        let result = parse(r"\vec{v}");
        assert!(matches!(result, MathNode::Accent { accent_type: AccentType::Vec, .. }));
    }

    // ── Espaçamentos ──────────────────────────────────────────────────────────

    #[test]
    fn thin_space() {
        // pulldown-latex: 3/18 em como f32; aproximamos para 1/6 em
        let node = parse(r"\,");
        match node {
            MathNode::Space(s) => {
                assert!((s - 1.0_f64 / 6.0).abs() < 1e-5, "esperava ~1/6 em, got {s}");
            }
            other => panic!("esperava Space, got {other:?}"),
        }
    }

    #[test]
    fn quad_space() {
        assert_eq!(parse(r"\quad"), MathNode::Space(1.0));
    }

    #[test]
    fn qquad_space() {
        assert_eq!(parse(r"\qquad"), MathNode::Space(2.0));
    }

    // ── Matriz ────────────────────────────────────────────────────────────────

    #[test]
    fn pmatrix_2x2() {
        let result = parse(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}");
        match result {
            MathNode::Matrix { rows, delimiters } => {
                assert_eq!(rows.len(), 2, "deve ter 2 fileiras");
                assert_eq!(rows[0].len(), 2, "fileira 0 deve ter 2 células");
                assert_eq!(rows[1].len(), 2, "fileira 1 deve ter 2 células");
                assert_eq!(delimiters, ("(".to_string(), ")".to_string()));
            }
            other => panic!("esperava Matrix, got {other:?}"),
        }
    }

    #[test]
    fn bmatrix_delimiters() {
        let result = parse(r"\begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix}");
        assert!(
            matches!(result, MathNode::Matrix { delimiters: (ref l, ref r), .. } if l == "[" && r == "]")
        );
    }

    #[test]
    fn vmatrix_delimiters() {
        let result = parse(r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}");
        assert!(
            matches!(result, MathNode::Matrix { delimiters: (ref l, ref r), .. } if l == "|" && r == "|")
        );
    }

    // ── Expressões de provas reais ────────────────────────────────────────────

    #[test]
    fn quadratic_formula() {
        let result = parse(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}");
        assert!(matches!(result, MathNode::Row(_)));
    }

    #[test]
    fn eulers_identity() {
        let result = parse(r"e^{i\pi} + 1 = 0");
        match result {
            MathNode::Row(nodes) => {
                assert!(matches!(nodes[0], MathNode::Superscript { .. }));
            }
            other => panic!("esperava Row, got {other:?}"),
        }
    }

    #[test]
    fn limit_expression() {
        let result = parse(r"\lim_{x \to 0} \frac{\sin x}{x} = 1");
        match result {
            MathNode::Row(nodes) => {
                assert!(
                    matches!(&nodes[0], MathNode::Subscript { base, .. }
                        if matches!(**base, MathNode::Operator(ref s) if s == "lim"))
                );
                assert!(matches!(&nodes[1], MathNode::Fraction { .. }));
            }
            other => panic!("esperava Row, got {other:?}"),
        }
    }

    #[test]
    fn derivative_expression() {
        parse(r"\frac{d}{dx}\left( x^n \right) = nx^{n-1}");
    }

    #[test]
    fn gaussian_integral() {
        let result = parse(r"\int_{-\infty}^{+\infty} e^{-x^2} dx = \sqrt{\pi}");
        assert!(matches!(result, MathNode::Row(_)));
    }

    #[test]
    fn taylor_series() {
        parse(r"f(x) = \sum_{n=0}^{\infty} \frac{f^{(n)}(a)}{n!}(x-a)^n");
    }

    #[test]
    fn vector_dot_product() {
        let result = parse(r"\vec{u} \cdot \vec{v} = \sum_{i=1}^{n} u_i v_i");
        assert!(matches!(result, MathNode::Row(_)));
    }

    #[test]
    fn matrix_determinant() {
        parse(r"\det(A) = \begin{vmatrix} a & b \\ c & d \end{vmatrix} = ad - bc");
    }

    #[test]
    fn binomial_coefficient() {
        parse(r"\frac{n!}{k!(n-k)!}");
    }

    #[test]
    fn pythagoras() {
        let result = parse(r"a^2 + b^2 = c^2");
        assert!(matches!(result, MathNode::Row(_)));
    }

    #[test]
    fn complex_fraction() {
        let result = parse(r"\frac{\partial f}{\partial x}");
        assert!(matches!(result, MathNode::Fraction { .. }));
    }

    // ── Erros ─────────────────────────────────────────────────────────────────

    #[test]
    fn error_unclosed_group() {
        let result = parse_latex(r"\frac{1}{2");
        assert!(result.is_err(), "grupo não fechado deve retornar Err");
    }

    #[test]
    fn error_missing_frac_arg() {
        let result = parse_latex(r"\frac{1}");
        assert!(result.is_err(), "frac sem segundo argumento deve retornar Err");
    }

    #[test]
    fn error_right_without_left() {
        let result = parse_latex(r"\right)");
        assert!(result.is_err(), "\\right sem \\left deve retornar Err");
    }

    #[test]
    fn error_has_position() {
        let result = parse_latex(r"\frac{1}{2");
        let err = result.unwrap_err();
        assert!(err.position > 0, "posição do erro deve ser > 0");
    }

    // ── Casos limítrofes ──────────────────────────────────────────────────────

    #[test]
    fn empty_input() {
        let result = parse_latex("");
        assert!(result.is_ok(), "input vazio deve parsear sem erro");
    }

    #[test]
    fn whitespace_only() {
        let result = parse_latex("   ");
        assert!(result.is_ok());
    }

    #[test]
    fn unknown_command_returns_error() {
        // pulldown-latex retorna Err para comandos desconhecidos
        let result = parse_latex(r"\unknowncommand");
        assert!(result.is_err(), "comando desconhecido deve retornar Err");
    }

    #[test]
    fn deeply_nested_superscripts() {
        parse(r"a^{b^{c^d}}");
    }

    #[test]
    fn function_with_subscript() {
        let result = parse(r"\lim_{x\to\infty}");
        assert!(matches!(result, MathNode::Subscript { .. }));
    }

    // ── MathML ────────────────────────────────────────────────────────────────

    #[test]
    fn mathml_basic() {
        let result = latex_to_mathml(r"x^2 + y^2 = r^2");
        assert!(result.is_ok(), "mathml gerado sem erro");
        let ml = result.unwrap();
        assert!(ml.contains("math"), "saída deve conter elemento math");
    }

    #[test]
    fn mathml_frac() {
        let result = latex_to_mathml(r"\frac{1}{2}");
        assert!(result.is_ok());
        let ml = result.unwrap();
        assert!(ml.contains("mfrac"), "fração deve gerar mfrac");
    }
}
