# PROJECT.md — prova-pdf

Especificação completa do projeto: requisitos de negócio, schema JSON, regras de comportamento, stack tecnológica e metas de qualidade.

---

## Índice

1. [Visão geral](#1-visão-geral)
2. [Requisitos de negócio](#2-requisitos-de-negócio)
3. [Schema JSON — ExamSpec](#3-schema-json--examspec)
4. [Tipos de questão](#4-tipos-de-questão)
5. [Textos-base (BaseText)](#5-textos-base-basetext)
6. [Cabeçalho institucional](#6-cabeçalho-institucional)
7. [Sistema de fontes](#7-sistema-de-fontes)
8. [PrintConfig — referência completa](#8-printconfig--referência-completa)
9. [Stack tecnológica](#9-stack-tecnológica)
10. [Metas de performance e tamanho](#10-metas-de-performance-e-tamanho)
11. [Apêndice: Mapeamento lizeedu → prova-pdf](#11-apêndice-mapeamento-lizeedu--prova-pdf)

---

## 1. Visão geral

`prova-pdf` é uma biblioteca Rust compilada para **WebAssembly** que gera PDFs de provas acadêmicas a partir de um JSON estruturado (`ExamSpec`). É construída especificamente para o domínio de avaliações educacionais, com suporte nativo a todos os tipos de questão, textos-base, cabeçalho institucional, fontes nomeadas e layout bicolunado.

**Princípio central:** `ExamSpec` JSON entra, `Uint8Array` de bytes PDF sai. Zero dependência de DOM, browser, canvas ou serviço externo.

**Diferença do benchmark (`exam-pdf`):** Enquanto `exam-pdf` é uma biblioteca genérica de PDF, `prova-pdf` é domain-specific: o schema modela explicitamente seções, questões com tipos concretos, espaços de resposta tipados e cabeçalho institucional. Isso elimina todas as lacunas identificadas na análise de migração.

**Targets:**
- `wasm32-unknown-unknown` + wasm-bindgen → pacote npm (browser / Node.js)
- `wasm32-wasip1` → WASI C-ABI para Python (wasmtime) e Go (wazero)

---

## 2. Requisitos de negócio

Derivados da análise do `ExamPrintView` do lizeedu e do modelo `Question`.

### 2.1 Configurações de página e layout

| Requisito | Fonte | Detalhe |
|---|---|---|
| Tamanho A4 | padrão | 210×297mm |
| Tamanho ATA | SEDUC | 200×266mm |
| Tamanho customizado | geral | largura × altura em mm |
| 1 coluna | padrão | |
| 2 colunas | `two_columns` | gutter de 8pt entre colunas, linha divisória 0.5pt |
| Margens configuráveis | `margin_*` | top/bottom/left/right em cm |
| Quebra de página forçada por questão | `force_break_page` | campo do modelo Question |
| Quebra de página antes de todas as questões | `break_all_questions` | param global |
| Modo economia | `economy_mode` | força 2 colunas, remove espaços de resposta |

### 2.2 Tipografia

| Requisito | Fonte | Detalhe |
|---|---|---|
| Família de fonte configurável | `font_family` | ibm/verdana/times/arial (por nome) |
| Tamanho de fonte configurável | `font_size` | 6–16pt |
| Espaçamento de linha | `line_spacing` | Normal/1.5×/2.5×/3.5× |
| Negrito, itálico, sublinhado inline | inline HTML | via InlineText.style |
| Subscrito, sobrescrito | `<sub>`, `<sup>` | InlineContent::Sub/Sup |
| Math LaTeX inline e display | MathJax markers | `\(...\)` e `\[...\]` |
| Cor de texto configurável por elemento | style.color | hex string `#RRGGBB` |
| Modo all_black | `all_black` | força todas as cores para preto (modo SEDUC) |

### 2.3 Tipos de questão

| Tipo | Código | Descrição |
|---|---|---|
| Objetiva | `choice` | Alternativas A/B/C/D/E com conteúdo inline |
| Discursiva | `textual` | Linhas ou espaço em branco para resposta |
| Somatório | `sum` | Itens com valores 01/02/04/08/16... e caixa de soma |
| Cloze | `cloze` | Lacunas inline no enunciado + banco de palavras |
| Redação | `essay` | Espaço grande com muitas linhas |
| Arquivo | `file` | Apenas enunciado + placeholder de upload |

### 2.4 Espaço de resposta discursiva

| Modo | `discursive_space_type` | Comportamento |
|---|---|---|
| Linhas | `lines` | N linhas com altura configurável (default 0.8cm) |
| Espaço em branco | `blank` | Caixa vazia com borda |
| Sem borda | `noBorder` | Espaço vertical sem marcação visual |

### 2.5 Textos-base (material de apoio)

| Posição | Descrição |
|---|---|
| `beforeQuestion` | Bloco antes do enunciado (largura total) |
| `afterQuestion` | Bloco após o espaço de resposta |
| `leftOfQuestion` | Coluna esquerda; questão à direita (mini 2-col) |
| `rightOfQuestion` | Coluna direita; questão à esquerda |
| `sectionTop` | Antes de todas as questões da seção |
| `examTop` | Antes de todas as seções (início do documento) |
| `examBottom` | Após todas as seções (fim do documento) |

### 2.6 Cabeçalho institucional

Renderizado no topo da página 1:
- Nome da instituição/escola
- Título da prova, disciplina, ano letivo
- Logo (imagem registrada por chave)
- Campos de preenchimento: Nome, Turma, Data, Nota (configuráveis)
- Bloco de instruções
- Header/footer corrente em páginas 2+ (`{page}/{pages}`)

### 2.7 Comportamentos de impressão

| Comportamento | Param | Detalhe |
|---|---|---|
| Exibir pontuação | `show_score` | Renderizar pontos junto ao número da questão |
| Ocultar numeração | `hide_numbering` | Global |
| Ocultar número por questão | `show_number: false` | Por questão |
| Área de rascunho | `draft_lines` | N linhas com borda, após espaço de resposta |
| Altura das linhas de rascunho | `draft_line_height` | cm |
| Questão span total (2-col) | `full_width` | Questão ocupa as 2 colunas |
| Imagens em escala de cinza | `image_grayscale` | Converte antes de embedar |
| Separar por disciplina | seção com título | Via structure de sections |

---

## 3. Schema JSON — ExamSpec

Estrutura completa passada ao `generate_pdf()`.

```typescript
interface ExamSpec {
  metadata?: ExamMetadata;
  config?: PrintConfig;      // configurações de impressão
  header?: InstitutionalHeader;
  sections: Section[];       // grupos de questões
  appendix?: Appendix;       // apêndice: fórmulas, textos extras
}

interface ExamMetadata {
  title?: string;
  author?: string;
  subject?: string;
  date?: string;
  keywords?: string[];
}

// ── PrintConfig ───────────────────────────────────────────────────────────────

interface PrintConfig {
  // Página
  pageSize?: "A4" | "Ata" | { widthMm: number; heightMm: number };
  margins?: { top: number; bottom: number; left: number; right: number }; // cm
  columns?: 1 | 2;

  // Tipografia
  fontSize?: number;                // pt, default 12
  lineSpacing?: "normal" | "oneAndHalf" | "twoAndHalf" | "threeAndHalf";
  fontFamily?: string;              // nome registrado no FontRegistry, default "body"

  // Espaços de resposta
  discursiveLineHeight?: number;    // cm por linha, default 0.8
  discursiveSpaceType?: "lines" | "blank" | "noBorder";

  // Flags de comportamento
  economyMode?: boolean;            // remove espaços, força 2 colunas
  breakAllQuestions?: boolean;      // pageBreak antes de cada questão
  imageGrayscale?: boolean;
  allBlack?: boolean;               // força cores preto (SEDUC)

  // Flags de exibição
  showScore?: boolean;
  hideNumbering?: boolean;
  headerFull?: boolean;             // exibir campos de aluno no cabeçalho
}

// ── Section ───────────────────────────────────────────────────────────────────

interface Section {
  title?: string;                   // heading da seção
  instructions?: InlineContent[];   // instruções após o heading
  questions: Question[];
  category?: string;                // para agrupamento por categoria
  style?: Style;
  forcePageBreak?: boolean;
}

// ── Question ──────────────────────────────────────────────────────────────────

interface Question {
  kind: "choice" | "textual" | "cloze" | "sum" | "essay" | "file";
  stem: InlineContent[];            // enunciado
  answer: AnswerSpace;              // espaço de resposta compatível com kind
  number?: number;                  // número de exibição (auto se ausente)
  label?: string;                   // override do número
  baseTexts?: BaseText[];
  points?: number;
  fullWidth?: boolean;              // span 2 colunas, default false
  draftLines?: number;              // linhas de rascunho, default 0
  draftLineHeight?: number;         // cm por linha de rascunho
  showNumber?: boolean;             // default true
  forcePageBreak?: boolean;
  style?: Style;
}

// ── AnswerSpace (discriminated by type) ───────────────────────────────────────

type AnswerSpace =
  | { type: "choice";  alternatives: Alternative[]; layout?: "vertical" | "horizontal" }
  | { type: "textual"; lineCount?: number; blankHeightCm?: number; lineHeightCm?: number }
  | { type: "cloze";   wordBank: InlineContent[][]; shuffleDisplay?: boolean }
  | { type: "sum";     items: SumItem[]; showSumBox?: boolean }
  | { type: "essay";   lineCount?: number; heightCm?: number }
  | { type: "file";    label?: string }

interface Alternative {
  label: string;                    // "A"/"B"/"C" ou "01"/"02"/"04"
  content: InlineContent[];
}

interface SumItem {
  value: number;                    // 1, 2, 4, 8, 16, 32, 64
  content: InlineContent[];
}

// ── BaseText ──────────────────────────────────────────────────────────────────

interface BaseText {
  content: InlineContent[];
  position: "beforeQuestion" | "afterQuestion" | "leftOfQuestion" |
            "rightOfQuestion" | "sectionTop" | "examTop" | "examBottom";
  title?: string;                   // "Texto I", "Figura 1"
  attribution?: string;             // "SARAMAGO, José. A Jangada de Pedra, 2023."
  style?: Style;
}

// ── InstitutionalHeader ───────────────────────────────────────────────────────

interface InstitutionalHeader {
  institution?: string;
  title?: string;
  subject?: string;
  year?: string;
  logoKey?: string;                 // chave registrada via add_image()
  studentFields?: StudentField[];
  runningHeader?: RunningHeader;    // cabeçalho corrente (pág 2+)
  runningFooter?: RunningHeader;
  instructions?: InlineContent[];
}

interface StudentField {
  label: string;                    // "Nome", "Turma", "Data", "Nota"
  widthCm?: number;                 // largura da linha; null = preenche o restante
}

interface RunningHeader {
  left?: string;   // suporta {page} e {pages}
  center?: string;
  right?: string;
}

// ── InlineContent ─────────────────────────────────────────────────────────────

type InlineContent =
  | { type: "text";  value: string; style?: InlineStyle }
  | { type: "math";  latex: string; display?: boolean }
  | { type: "image"; key: string; widthCm?: number; heightCm?: number; caption?: string }
  | { type: "sub";   content: InlineContent[] }
  | { type: "sup";   content: InlineContent[] }
  | { type: "blank"; widthCm?: number }      // lacuna para cloze (default 3.5cm)

interface InlineStyle {
  fontFamily?: string;
  fontSize?: number;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  color?: string;                   // "#RRGGBB"
  backgroundColor?: string;
}

// ── Appendix ──────────────────────────────────────────────────────────────────

interface Appendix {
  title?: string;
  content: AppendixItem[];
}

type AppendixItem =
  | { type: "block";        content: InlineContent[]; title?: string; style?: Style }
  | { type: "formulaSheet"; title?: string; formulas: FormulaEntry[] }
  | { type: "pageBreak" }

interface FormulaEntry {
  label?: string;
  latex: string;
}

// ── Style ─────────────────────────────────────────────────────────────────────

interface Style {
  fontFamily?: string;
  fontSize?: number;
  fontWeight?: "normal" | "bold";
  fontStyle?: "normal" | "italic";
  color?: string;
  backgroundColor?: string;
  underline?: boolean;
  textAlign?: "left" | "center" | "right" | "justified";
}
```

---

## 4. Tipos de questão

### 4.1 Choice (objetiva)

Alternativas com label e conteúdo inline. Labels tipicamente A/B/C/D/E para múltipla escolha padrão.

```json
{
  "kind": "choice",
  "stem": [{ "type": "text", "value": "Qual é a capital do Brasil?" }],
  "answer": {
    "type": "choice",
    "alternatives": [
      { "label": "A", "content": [{ "type": "text", "value": "São Paulo" }] },
      { "label": "B", "content": [{ "type": "text", "value": "Brasília" }] },
      { "label": "C", "content": [{ "type": "text", "value": "Rio de Janeiro" }] }
    ]
  },
  "points": 1.0
}
```

### 4.2 Textual (discursiva)

Espaço de resposta com linhas ou caixa em branco.

```json
{
  "kind": "textual",
  "stem": [{ "type": "text", "value": "Explique o teorema de Pitágoras." }],
  "answer": { "type": "textual", "lineCount": 8, "lineHeightCm": 1.0 },
  "draftLines": 3,
  "fullWidth": true
}
```

### 4.3 Sum (somatório)

Itens com valores binários (01, 02, 04, 08, 16, 32, 64). O aluno soma os valores dos itens corretos.

```json
{
  "kind": "sum",
  "stem": [{ "type": "text", "value": "Marque os itens corretos e some os valores." }],
  "answer": {
    "type": "sum",
    "items": [
      { "value": 1,  "content": [{ "type": "text", "value": "A Terra é redonda." }] },
      { "value": 2,  "content": [{ "type": "text", "value": "O Sol é uma estrela." }] },
      { "value": 4,  "content": [{ "type": "text", "value": "A Lua é um planeta." }] },
      { "value": 8,  "content": [{ "type": "text", "value": "O DNA contém genes." }] }
    ],
    "showSumBox": true
  }
}
```

### 4.4 Cloze (preenchimento de lacunas)

O enunciado (`stem`) contém `InlineContent::Blank` nas posições das lacunas. A `answer` fornece o banco de palavras.

```json
{
  "kind": "cloze",
  "stem": [
    { "type": "text", "value": "A " },
    { "type": "blank", "widthCm": 3.0 },
    { "type": "text", "value": " é a capital do Brasil, localizada no " },
    { "type": "blank", "widthCm": 2.5 },
    { "type": "text", "value": "." }
  ],
  "answer": {
    "type": "cloze",
    "wordBank": [
      [{ "type": "text", "value": "Brasília" }],
      [{ "type": "text", "value": "Distrito Federal" }],
      [{ "type": "text", "value": "São Paulo" }]
    ]
  }
}
```

### 4.5 Essay (redação)

Espaço grande com título do tema e muitas linhas.

```json
{
  "kind": "essay",
  "stem": [{ "type": "text", "value": "Tema: Redija uma dissertação sobre sustentabilidade." }],
  "answer": { "type": "essay", "lineCount": 30 },
  "fullWidth": true
}
```

### 4.6 File (envio de arquivo)

Apenas enunciado + placeholder de instrução para upload digital.

```json
{
  "kind": "file",
  "stem": [{ "type": "text", "value": "Grave um vídeo explicando o fenômeno." }],
  "answer": { "type": "file", "label": "Envie o arquivo pela plataforma até a data indicada." }
}
```

---

## 5. Textos-base (BaseText)

### Posições suportadas

**`beforeQuestion` / `afterQuestion`** — bloco de largura total antes ou após a questão:

```
┌─────────────────────────────────────────────────────────┐
│ [Texto-base: Leia o trecho a seguir...]                 │
│ "Era uma vez um rei muito sábio..."                     │
│ (Fonte: Conto Popular Brasileiro, 2023)                 │
├─────────────────────────────────────────────────────────┤
│ 1. Com base no texto, responda:                         │
│ ________________________________________                │
└─────────────────────────────────────────────────────────┘
```

**`leftOfQuestion` / `rightOfQuestion`** — mini layout bicolunado dentro da questão:

```
┌───────────────────────┬─────────────────────────────────┐
│ [Texto-base]          │ 3. A que personagem o texto se  │
│ "O menino correu..."  │    refere? Explique.             │
│                       │ ____________________________    │
│ (SARAMAGO, 2019)      │ ____________________________    │
└───────────────────────┴─────────────────────────────────┘
```

**`sectionTop`** — renderizado antes das questões da seção (usado para textos que referenciados por múltiplas questões seguintes).

**`examTop` / `examBottom`** — renderizado no início/fim do documento, independente de qualquer questão.

---

## 6. Cabeçalho institucional

```
┌───────────────────────────────────────────────────────────────┐
│  [LOGO]   ESCOLA ESTADUAL JOÃO PESSOA                         │
│           Avaliação Bimestral de Matemática — 1º Bimestre     │
│           Turma: 8º Ano B  |  Ano Letivo: 2026                │
├───────────────────────────────────────────────────────────────┤
│  Nome: ___________________________________ Nº: _______________│
│  Turma: ____________  Data: ____________  Nota: _____________  │
├───────────────────────────────────────────────────────────────┤
│  INSTRUÇÕES: Responda com caneta azul ou preta. Não rasure.   │
└───────────────────────────────────────────────────────────────┘
```

- Logo à esquerda (opcional, registrado via `add_image("logo", bytes)`)
- Campos de aluno renderizados como `label: ___________` com `widthCm` configurável
- Linha separadora (HorizontalRule) após os campos
- Instruções como `InlineContent[]` (podem conter math, bold, etc.)

---

## 7. Sistema de fontes

### 7.1 Registro por nome

```typescript
// Registrar família "body" (obrigatório)
add_font("body", 0, regularFontBytes);    // variant 0 = regular
add_font("body", 1, boldFontBytes);       // variant 1 = bold
add_font("body", 2, italicFontBytes);     // variant 2 = italic
add_font("body", 3, boldItalicBytes);     // variant 3 = bold-italic

// Registrar família adicional para headings
add_font("heading", 0, verdanaBytes);
add_font("heading", 1, verdanaBoldBytes);
```

### 7.2 FontRules — mapeamento de papel → família

Configurado opcionalmente na spec ou via API separada:

```typescript
set_font_rules({
  body:     "IBM Plex Sans",   // texto de corpo (padrão)
  heading:  "Verdana",         // títulos de seção
  question: "IBM Plex Sans",   // número e pontos da questão
  math:     "IBM Plex Sans",   // texto matemático não-WASM
})
```

Se não configurado: todas as roles usam a família `"body"`.

### 7.3 Cascata de resolução

```
Style.fontFamily (por elemento)
    ↓ se ausente
FontRules[role] (por papel: body/heading/question/math)
    ↓ se ausente
"body" (família padrão)
    ↓ se ausente
Primeira família registrada
    ↓ se vazio
PipelineError::NoFont
```

### 7.4 Fallback de variante

Se `bold` não está registrada → usa `regular` da mesma família.
Se `bold-italic` não está registrada → usa `bold`, depois `regular`.

---

## 8. PrintConfig — referência completa

| Campo | Tipo | Default | Descrição |
|---|---|---|---|
| `pageSize` | enum | `"A4"` | A4, Ata (200×266mm), Custom |
| `margins.top` | f64 cm | `2.5` | Margem superior |
| `margins.bottom` | f64 cm | `2.5` | Margem inferior |
| `margins.left` | f64 cm | `2.5` | Margem esquerda |
| `margins.right` | f64 cm | `2.5` | Margem direita |
| `columns` | u8 | `1` | Número de colunas (1 ou 2) |
| `fontSize` | f64 pt | `12.0` | Tamanho base da fonte |
| `lineSpacing` | enum | `"normal"` | Espaçamento entre linhas: 1.4×/1.5×/2.5×/3.5× |
| `fontFamily` | String | `"body"` | Família padrão do corpo |
| `discursiveLineHeight` | f64 cm | `0.8` | Altura de cada linha de resposta |
| `discursiveSpaceType` | enum | `"lines"` | Tipo de espaço de resposta |
| `economyMode` | bool | `false` | Remove espaços; força 2 colunas |
| `breakAllQuestions` | bool | `false` | PageBreak antes de cada questão |
| `imageGrayscale` | bool | `false` | Imagens em escala de cinza |
| `allBlack` | bool | `false` | Força todas as cores para preto |
| `showScore` | bool | `false` | Exibe pontuação junto ao número |
| `hideNumbering` | bool | `false` | Oculta números de questão globalmente |
| `headerFull` | bool | `true` | Exibe campos de aluno no cabeçalho |

---

## 9. Stack tecnológica

### 9.1 Crates core (PDF + tipografia)

| Crate | Versão | Propósito | Justificativa |
|---|---|---|---|
| `pdf-writer` | 0.14 | Emissão de bytes PDF | Mesma stack do Typst; zero dependências inseguras; WASM-safe |
| `ttf-parser` | 0.25 | Parsing TTF/OTF + tabela MATH | Referência para fontes em Rust puro; suporta OpenType MATH para math layout |
| `rustybuzz` | 0.20 | Shaping de texto OpenType | Port puro Rust do HarfBuzz; kerning, ligatures, glyph substitution |
| `subsetter` | 0.2 | Subsetting de fontes | Reduz tamanho do PDF: embarca apenas os glifos usados |
| `unicode-linebreak` | 0.1 | Quebra de linha UAX #14 | Correto por spec Unicode; tratamento de espaços, hífens, ideogramas |
| `miniz_oxide` | 0.8 | Compressão Deflate | Comprime streams de fontes e imagens no PDF |
| `thiserror` | 2 | Tipos de erro ergonômicos | Mensagens claras sem boilerplate |

### 9.2 Crates de bridge (WASM)

| Crate | Versão | Propósito |
|---|---|---|
| `wasm-bindgen` | 0.2 | Interoperabilidade JS ↔ Rust |
| `serde` | 1.0 | Serialização/deserialização |
| `serde-wasm-bindgen` | 0.6 | Conversão direta JsValue ↔ structs |
| `js-sys` | 0.3 | Bindings para tipos JS nativos |
| `serde_json` | 1 | Parse JSON no target WASI |

### 9.3 Crates opcionais (feature flags)

| Crate | Feature | Propósito |
|---|---|---|
| `pulldown-latex` | `math` | Parser LaTeX → AST + layout OpenType MATH |
| `image` | `images` | Decodificação PNG e JPEG |

### 9.4 Ferramentas de build

| Ferramenta | Propósito |
|---|---|
| `wasm-bindgen-cli` | Geração dos bindings JS a partir do `.wasm` |
| `wasm-opt` (binaryen) | Otimização de tamanho com `-Oz` |
| `wasmtime` | Runtime WASI para testes Python |
| `wazero` | Runtime WASI para testes Go |

### 9.5 Decisões de design (benchmark learnings)

| Decisão | Alternativa descartada | Razão |
|---|---|---|
| `pdf-writer` | `printpdf` | Mesma stack do Typst; zero unsafe; não requer build scripts |
| Math nativo (pulldown-latex + MATH table) | KaTeX JS | Texto selecionável no PDF; sem cruzamento de fronteira JS; correto tipograficamente |
| Engine de layout próprio | `cosmic-text` | Sem rasterização desnecessária; tamanho menor no WASM |
| Enum + match exaustivo | trait objects | Compilador força tratamento de todos os casos; sem vtable; melhor inlining |
| `panic = "abort"` no release | unwind | Sem tabelas de unwind = binário menor; seguro pois panics só existem em tests |
| `FontRegistry` nomeado | Variante única por índice hardcoded | Suporta múltiplas famílias; resolve sem lógica no chamador |

---

## 10. Metas de performance e tamanho

### Tamanho do binário WASM

| Configuração | Meta gzipped |
|---|---|
| Com math + images (padrão) | < 900KB |
| Sem images (`--no-default-features --features browser,math`) | < 750KB |
| Sem math e images (mínimo) | < 500KB |

Contexto do benchmark: exam-pdf atingiu 737KB (browser) e 770KB (WASI) com math+images.

### Performance de geração

| Prova | Meta |
|---|---|
| 50 questões, sem math | < 50ms |
| 50 questões, com LaTeX | < 200ms |
| Prova completa com imagens | < 400ms |

Contexto: o fluxo atual (Chromium headless) leva 8–15 segundos para a mesma prova.

### Qualidade visual

SSIM mínimo (Structural Similarity) comparado ao PDF gerado pelo Chromium:
- Páginas de texto puro: SSIM ≥ 0.96
- Páginas com math: SSIM ≥ 0.90 (engines diferentes produzem resultado ligeiramente diferente)
- Meta geral: SSIM médio ≥ 0.92 por prova

---

## 11. Apêndice: Mapeamento lizeedu → prova-pdf

### GET params do ExamPrintView → PrintConfig

| Param Django | Campo prova-pdf | Observação |
|---|---|---|
| `paper_size=a4` | `pageSize: "A4"` | |
| `paper_size=ata` | `pageSize: "Ata"` | 200×266mm |
| `two_columns=1` | `columns: 2` | |
| `font_size=12` | `fontSize: 12` | |
| `font_family=ibm` | `fontFamily: "body"` + `add_font("body", ...)` | |
| `line_spacing=1` | `lineSpacing: "oneAndHalf"` | 0=normal, 1=1.5×, 2=2.5×, 3=3.5× |
| `discursive_line_height=1.0` | `discursiveLineHeight: 1.0` | cm |
| `economy_mode=1` | `economyMode: true` | |
| `break_all_questions=1` | `breakAllQuestions: true` | |
| `print_images_with_grayscale=1` | `imageGrayscale: true` | |
| `all_black=1` | `allBlack: true` | |
| `show_question_score=1` | `showScore: true` | |
| `hide_numbering=1` | `hideNumbering: true` | |
| `margin_top=0.6` | `margins.top: 0.6` | |

### Modelos Django → schema prova-pdf

| Modelo Django | Schema prova-pdf |
|---|---|
| `Exam` | `ExamSpec` |
| `ExamQuestion` (ordenado) | `Section.questions[]` |
| `Question.enunciation` (HTML) | `Question.stem` (InlineContent[]) |
| `Question.category == CHOICE` | `kind: "choice"` |
| `Question.category == TEXTUAL` | `kind: "textual"` |
| `Question.category == SUM_QUESTION` | `kind: "sum"` |
| `Question.category == CLOZE` | `kind: "cloze"` |
| `Question.is_essay == True` | `kind: "essay"` |
| `Question.category == FILE` | `kind: "file"` |
| `QuestionOption.text` (HTML) | `Alternative.content` (InlineContent[]) |
| `Question.quantity_lines` | `TextualAnswer.lineCount` |
| `Question.text_question_format` | `TextualAnswer` (lines vs blank) |
| `Question.draft_rows_number` | `Question.draftLines` |
| `Question.force_one_column` | `Question.fullWidth: true` |
| `Question.force_break_page` | `Question.forcePageBreak: true` |
| `Question.number_is_hidden` | `Question.showNumber: false` |
| `Question.base_texts` | `Question.baseTexts[]` |
| `BaseText.position` | `BaseTextPosition` enum |
