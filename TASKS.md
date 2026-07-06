# prova-pdf — Tasks

Projeto: gerador de PDF de provas em Rust/WASM, substituto do Chromium headless na lize.

**Legenda de status:** `[ ]` pendente · `[x]` concluído · `[~]` em progresso

---

## Fase 0 — Scaffold e spec (concluída)

### TASK-001 — Cargo.toml e estrutura de crates `[x]`
Criar o projeto Rust com features `browser`, `wasi-lib`, `math`, `images`.
Deps: pdf-writer 0.14, ttf-parser 0.25, rustybuzz 0.20, subsetter 0.2,
unicode-linebreak 0.1, miniz_oxide 0.8, thiserror 2, serde/serde_json.
Profile release: opt-level=z, lto=true, codegen-units=1, strip=true, panic=abort.

### TASK-002 — ExamSpec schema completo `[x]`
Implementar `src/spec/` com todos os módulos:
- `exam.rs`: ExamSpec, Section, Appendix, AppendixItem
- `question.rs`: Question, QuestionKind, BaseText, BaseTextPosition (7 posições)
- `answer.rs`: AnswerSpace discriminada (Choice/Textual/Cloze/Sum/Essay/File)
- `inline.rs`: InlineContent com Sub/Sup recursivos e Blank
- `header.rs`: InstitutionalHeader, StudentField, RunningHeader
- `config.rs`: PrintConfig com todos os 30+ parâmetros do ExamPrintView
- `style.rs`: Style cascadeável, FontWeight, FontStyle

**Critério:** todos os roundtrips JSON passam; zero warnings.

### TASK-003 — Sistema de fontes (FontRegistry + FontRules + FontResolver) `[x]`
Implementar `src/fonts/`:
- `data.rs`: FontData (bytes + OwnedFace), FontFamily, FontData::empty/is_empty
- `registry.rs`: FontRegistry, add_variant (valida índice ANTES do parse), FontRules
- `resolve.rs`: FontResolver, FontRole, pick_variant (fallback chain)

**Critério:** testes unitários passam; `invalid_variant_returns_error` retorna `InvalidVariant(9)`.

### TASK-004 — Fragment IR `[x]`
Implementar `src/layout/fragment.rs` com Fragment, FragmentKind, GlyphRun,
HRule, FilledRect, StrokedRect, ImageFragment, Spacer.

### TASK-005 — Bindings stub: browser e WASI `[x]`
Implementar `src/bindings/browser.rs` e `src/bindings/wasi.rs` com a API
completa mas retornando erro "not yet implemented" no generate.
WASI usa `prova_pdf_*` com `#[unsafe(no_mangle)]` (Rust 2024).

### TASK-006 — Makefile, .gitignore, PROJECT.md, ARCHITECTURE.md `[x]`
- Makefile: build-browser, build-wasi, build-all, test, clean, size
- wasm-opt: `-Oz --strip-debug --strip-producers --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int`
- PROJECT.md e ARCHITECTURE.md com planejamento completo

---

## Fase 1 — Geometria de página e layout básico

### TASK-007 — PageGeometry a partir de PrintConfig `[x]`
Implementar `src/layout/page.rs`:

```rust
pub struct PageGeometry {
    pub page_width_pt:    f64,
    pub page_height_pt:   f64,
    pub margin_top_pt:    f64,
    pub margin_bottom_pt: f64,
    pub margin_left_pt:   f64,
    pub margin_right_pt:  f64,
    pub content_width_pt: f64,
    pub content_height_pt: f64,
    pub columns:          u8,
    pub column_gap_pt:    f64,
    pub column_width_pt:  f64,
}

impl PageGeometry {
    pub fn from_config(cfg: &PrintConfig) -> Self { … }
}
```

- `PageSize::A4` → 595.28 × 841.89 pt
- `PageSize::Ata` → 566.93 × 754.02 pt (200mm × 266mm)
- `PageSize::Custom(w_cm, h_cm)` → converte cm × 28.3465
- Margens: `cfg.margins.top_cm * 28.3465`
- `column_gap_pt = 14.0` (≈ 0.5cm)

**Critério:** testes parametrizados para A4, ATA, Custom; content_width_pt = page − margens.

### TASK-008 — PageComposer: empilhamento vertical e paginação `[x]`
Implementar `PageComposer` em `src/layout/page.rs`:

```rust
pub struct PageComposer<'a> {
    geometry:     PageGeometry,
    resolver:     FontResolver<'a>,
    images:       &'a HashMap<String, Vec<u8>>,
    config:       &'a PrintConfig,
    cursor_y:     f64,
    current_col:  u8,
    col_x_offset: f64,
    current_page: Vec<Fragment>,
    pages:        Vec<Vec<Fragment>>,
}
```

- `push_block(height, fragments)` → verifica overflow → new_page se necessário
- `new_page()` → flush current_page, reset cursor_y, reset col
- `finalize() -> Vec<Vec<Fragment>>` → flush última página
- `force_page_break` na Question → `new_page()` antes de adicionar
- `break_all_questions` no config → `new_page()` antes de cada questão
- Colunas: ao atingir metade da altura útil, `next_column()` → col_x_offset += column_width_pt + gap

**Critério:** testes com altura de bloco maior que página geram nova página; 2 colunas equilibram.

### TASK-009 — InlineLayoutEngine: shaping + quebra de linha `[x]`
Implementar `src/layout/inline.rs`:

```rust
pub struct InlineLayoutEngine<'a> {
    resolver:        &'a FontResolver<'a>,
    available_width: f64,
    font_size:       f64,
    line_spacing:    f64,
}

impl InlineLayoutEngine<'_> {
    pub fn layout(
        &self,
        content: &[InlineContent],
        role: FontRole,
        style: &ResolvedStyle,
        origin_x: f64,
        origin_y: f64,
    ) -> (Vec<Fragment>, f64 /* total height */) { … }
}
```

- rustybuzz::UnicodeBuffer + shape → glyph IDs, x_advances, x_offsets, y_offsets
- unicode-linebreak para oportunidades de quebra
- Greedy fill: acumula tokens até width overflow → nova linha
- Sub/Sup: font_size × 0.65, baseline ±0.35em, recursivo
- Blank: FilledRect com `width_cm.unwrap_or(3.5) * 28.3465`
- Math (feature "math"): delega para `MathLayout`

**Critério:** texto simples quebra no limite; Sub/Sup ajustam baseline; Blank tem largura correta.

---

## Fase 2 — Cascata de estilo e validação

### TASK-010 — ResolvedStyle e cascata PrintConfig → Section → Question → Inline `[x]`
Implementar `src/spec/style.rs` (complemento) e `src/pipeline/style.rs`:

```rust
pub struct ResolvedStyle {
    pub font_size:   f64,
    pub font_weight: FontWeight,
    pub font_style:  FontStyle,
    pub font_family: Option<String>,
    pub color:       (f32, f32, f32),
    pub underline:   bool,
    pub text_align:  TextAlign,
    pub line_spacing: f64,
}
```

Cascata: valor mais específico sobrescreve o mais geral.
Defaults do PrintConfig: font_size=12.0, color=(0,0,0).
`allBlack: true` → força color=(0,0,0) em toda a cascata.

**Critério:** testes de cascata; allBlack sobrescreve qualquer cor definida.

### TASK-011 — Fase 1 do pipeline: Validação `[x]`
Implementar `src/pipeline/validate.rs`:

- registry.is_ready() → PipelineError::NoFont
- Pelo menos 1 seção com pelo menos 1 questão
- Questões Choice: pelo menos 2 alternativas; todas com key única
- Imagens: todo `InlineContent::Image { key }` e `header.logo_key` presente no ImageStore
- `StudentField.width_cm` se presente deve ser > 0
- Retorna `Vec<ValidationError>` (não-fatal: reporta todos os erros)

**Critério:** fixtures com erros deliberados retornam os erros esperados.

---

## Fase 3 — Renderização do cabeçalho

### TASK-012 — Layout do InstitutionalHeader `[x]`
Implementar `src/layout/header.rs`:

```
layout_header(header, resolver, geometry, images) -> (Vec<Fragment>, f64)
```

- Logo: ImageFragment à esquerda, `logo_height_cm` ou 2.0cm padrão
- Texto à direita do logo: institution (bold, heading role), title (bold), subject · year
- Linha separadora (HRule) após os dados institucionais
- `student_fields`: FilledRect (underline) com `width_cm`, label como GlyphRun
- Campos em linha, quebram para nova linha se necessário
- `instructions`: InlineLayoutEngine com `body` role
- Altura total retornada para o PageComposer

**Critério:** fixture `full_header.json` gera fragments com posições corretas.

### TASK-013 — RunningHeader e rodapé de página `[x]`
Implementar renderização do `RunningHeader` como layer separado:

- Chamado pelo PdfEmitter após montar cada página
- Substituição de tokens: `{page}` → número da página, `{pages}` → total
- Três regiões: left (align left), center (align center), right (align right)
- y = margin_top / 2 (cabeçalho) ou page_height − margin_bottom / 2 (rodapé)
- Fonte: body role, font_size = 9pt

**Critério:** PDF com 3 páginas mostra "1/3", "2/3", "3/3" no rodapé.

---

## Fase 4 — Renderização de questões

### TASK-014 — Numeração de questões e bloco de questão `[x]`
Implementar o bloco base em `src/layout/question.rs`:

```rust
pub fn layout_question(
    q: &Question,
    number: u32,              // número sequencial global ou por seção
    resolver: &FontResolver,
    geometry: &ColumnGeometry,
    config: &PrintConfig,
) -> (Vec<Fragment>, f64) { … }
```

- `show_number: true` → prefixo "01." com formatação de question role
- `questionNumberingType`: global (1–N) ou por seção (1–N por seção)
- `economyMode: true` → reduz espaçamentos por 30%

### TASK-015 — QuestionKind::Choice `[x]`
Renderizar questão de múltipla escolha:

- Stem: InlineLayoutEngine (question role)
- Para cada alternativa: `letter) texto` em body role
- `layout: AlternativeLayout` — Vertical (uma por linha) ou Grid (N por linha)
- `allBlack: true` → sem cor nos bullets
- Letras: A, B, C, D, E (ou a, b, c, d, e dependendo de `letterCase`)
- Espaçamento entre alternativas: `alternative_spacing_cm` do config

**Critério:** 5 alternativas em grid 2×3 posicionadas corretamente.

### TASK-016 — QuestionKind::Textual `[x]`
Renderizar questão dissertativa com linhas:

- Stem: InlineLayoutEngine
- `line_count` linhas HRule com `line_height_cm` de espaçamento
- `discursiveSpaceType`:
  - Lines → HRule por linha
  - Blank → FilledRect (bordas, sem linhas internas)
  - NoBorder → apenas espaço vertical
- `discursiveLineHeight` do config como fallback de line_height_cm

**Critério:** 5 linhas com altura configurável; NoBorder sem regras.

### TASK-017 — QuestionKind::Cloze `[x]`
Renderizar questão lacunada:

- Stem: texto com `InlineContent::Blank` inline → FilledRect underline
- `word_bank`: se presente, renderiza abaixo do stem como lista de alternativas
- Blank sem largura explícita → 3.5cm
- `economyMode` → blanks 2.5cm

**Critério:** blanks aparecem inline no texto; word_bank separado abaixo.

### TASK-018 — QuestionKind::Sum `[x]`
Renderizar questão de somatório:

- Stem com descrição do enunciado
- Lista de `SumItem { label, content, value }`:
  - Checkbox quadrado (StrokedRect 0.4cm) + label + conteúdo inline
- `show_sum_box: true` → caixa "Soma: ___" ao final
- Valor de cada item exibido à direita (ex: "01", "02", "04", "08")

**Critério:** 5 itens + caixa de soma; valores alinhados à direita.

### TASK-019 — QuestionKind::Essay `[x]`
Renderizar questão discursiva longa:

- Stem inline
- Área de resposta:
  - `line_count` especificado → N linhas HRule
  - `height_cm` especificado → área em branco com altura fixa
  - Prioridade: height_cm > line_count

**Critério:** height_cm gera área de altura correta; line_count gera N linhas.

### TASK-020 — QuestionKind::File `[x]`
Renderizar questão de envio de arquivo:

- Stem inline
- Caixa com label de instrução (ex: "Anexe o arquivo no sistema")
- Border dashed (StrokedRect com dash)
- Ícone placeholder (FilledRect ou texto)

**Critério:** caixa com border dashed e label visível.

### TASK-021 — BaseText: posicionamento nas 7 posições `[x]`
Implementar `src/layout/base_text.rs`:

Renderizar `Vec<BaseText>` de uma questão nas posições corretas:

- `BeforeQuestion`: InlineLayoutEngine acima do stem
- `AfterQuestion`: abaixo do AnswerSpace
- `LeftOfQuestion`: coluna lateral esquerda (força full_width na questão)
- `RightOfQuestion`: coluna lateral direita (força full_width na questão)
- `SectionTop`: renderizado pelo layout de seção, não da questão
- `ExamTop`: adicionado ao início do PageComposer, antes do header
- `ExamBottom`: adicionado ao final da última página

**Critério:** fixture `base_text_positions.json` com 7 questões posiciona corretamente.

---

## Fase 5 — Layout avançado

### TASK-022 — full_width e 2 colunas `[x]`
Integrar `Question.full_width` no PageComposer:

- `full_width: true` → questão ocupa toda a largura da área de conteúdo
- Se estiver em modo 2 colunas: `column-span: all` equivalente
- Implementação: PageComposer verifica `full_width` antes de calcular `column_width_pt`
- Questões LeftOfQuestion e RightOfQuestion são implicitamente full_width

**Critério:** questão full_width em layout 2 colunas ocupa `content_width_pt`.

### TASK-023 — draft_lines por questão `[x]`
Renderizar linhas de rascunho abaixo de qualquer tipo de questão:

- `draft_lines: u32` → N linhas HRule com traço mais claro (gray #AAAAAA)
- `draft_line_height: Option<f64>` → altura das linhas (padrão: 0.7cm)
- Aparecem sempre após o AnswerSpace
- Label "Rascunho" acima (body role, font_size=8pt, italic)

**Critério:** questão com draft_lines=3 gera 3 linhas cinzas com label.

### TASK-024 — Appendix `[x]`
Renderizar `Appendix` ao final do documento:

- `AppendixItem::Block { content: Vec<InlineContent> }` → InlineLayoutEngine
- `AppendixItem::FormulaSheet { formulas: Vec<InlineContent> }` → Math layout (feature "math")
- `AppendixItem::PageBreak` → PageComposer.new_page()
- Título do Appendix em heading role, bold

**Critério:** appendix com 3 items de tipos diferentes em posições corretas.

### TASK-025 — Seções: título, instruções e category `[x]`
Renderizar cabeçalho de seção em `src/layout/section.rs`:

- `title`: heading role, bold, font_size * 1.2
- `instructions`: body role, italic
- `category`: badge (FilledRect background + GlyphRun) acima do título
- `force_page_break: true` → new_page() antes da seção
- `SectionTop` BaseTexts renderizados entre cabeçalho e primeira questão

**Critério:** seção com título, instruções e categoria renderiza em ordem.

---

## Fase 6 — Emissão PDF

### TASK-026 — PdfEmitter: estrutura básica `[x]`
Implementar `src/pdf/emit.rs`:

```rust
pub struct PdfEmitter<'a> {
    registry: &'a FontRegistry,
    images:   &'a HashMap<String, Vec<u8>>,
}

impl PdfEmitter<'_> {
    pub fn emit(
        &self,
        pages: Vec<Vec<Fragment>>,
        geometry: &PageGeometry,
    ) -> Result<Vec<u8>, PipelineError> { … }
}
```

- Usa `pdf-writer 0.14` (Pdf, Page, Content, Resources)
- Cria uma Page por `Vec<Fragment>`
- MediaBox a partir de `geometry`
- Coordenadas: converte `y` (origem no topo) para PDF (origem no fundo)
  `pdf_y = page_height_pt − fragment.y − fragment.height`

**Critério:** gera PDF válido (estrutura mínima) com N páginas.

### TASK-027 — Embedding e subsetting de fontes `[x]`
Implementar `src/pdf/fonts.rs`:

- Coletar todos os `GlyphRun` de uma página → conjunto de (family, variant, glyph_ids)
- Para cada combinação: `subsetter::subset(font_bytes, glyph_ids)` → subset bytes
- Embeddar como `FontDescriptor` + `CIDFont` + `Type0` (CIDFontType2)
- Construir `ToUnicode CMap` para mapeamento glyph_id → codepoint (copy-paste)
- Reusar objetos entre páginas se o conjunto de glifos é idêntico

**Critério:** PDF com texto copiável; arquivo menor que font original.

### TASK-028 — Emissão de GlyphRun `[x]`
Renderizar texto shaped no content stream:

- `BT ... ET` com `Tf`, `Tm` (posição + matriz), `TJ` (array de glifos com espaçamentos)
- x_advances: converter de font units para pts: `advance * font_size / units_per_em`
- x_offsets, y_offsets: aplicar como ajustes de posição
- Cor: `rg` (fill) com valores normalizados
- `underline: true` → traço abaixo da baseline (`RG` + `w` + `m/l/S`)

**Critério:** texto posicionado corretamente; caracteres especiais com kerning.

### TASK-029 — Emissão de formas geométricas `[x]`
Implementar `src/pdf/drawing.rs` com operadores PDF:

- `HRule` → `m x y l x2 y S` com stroke width e color
- `FilledRect` → `re f` com fill color
- `StrokedRect` → `re S` com stroke; `StrokedRect::dash` → `[on off] 0 d`
- Helpers: `set_stroke_color(r,g,b)`, `set_fill_color(r,g,b)`, `set_line_width(w)`
- Corrigir import: `crate::spec::style::{Border, BorderStyle}` (não crate::model)

**Critério:** retângulos com e sem dash renderizados corretamente.

### TASK-030 — Embedding de imagens `[x]`
Implementar `src/pdf/images.rs`:

- Detectar formato: JPEG (magic `FF D8`) → DCTDecode; PNG → deflate (miniz_oxide)
- PNG: decodificar com `image` crate (feature "images"), recomprimir raw pixels
- PDF XObject Image com Width, Height, ColorSpace, BitsPerComponent
- `Do` operator para renderizar na posição do ImageFragment
- Cache: embeddar cada key apenas uma vez (usa referência de objeto PDF)

**Critério:** PDF com JPEG e PNG inline; tamanhos corretos.

---

## Fase 7 — Wiring do pipeline

### TASK-031 — Pipeline completo `[x]`
Conectar as 4 fases em `src/pipeline.rs`:

```rust
pub fn render(spec: &ExamSpec, ctx: &RenderContext) -> Result<Vec<u8>, PipelineError> {
    // Fase 1
    validate(spec, ctx)?;
    // Fase 2
    let styles = cascade_styles(spec, &ctx.config);
    // Fase 3
    let resolver = FontResolver::new(&ctx.registry, &ctx.rules);
    let geometry = PageGeometry::from_config(&spec.config);
    let pages = layout_exam(spec, &styles, &resolver, &geometry, ctx)?;
    // Fase 4
    let emitter = PdfEmitter::new(&ctx.registry, &ctx.images);
    emitter.emit(pages, &geometry)
}
```

**Critério:** fixture `all_kinds.json` com fontes DejaVu → PDF válido e não-vazio.

### TASK-032 — Bindings browser: wasm-bindgen completo `[x]`
Atualizar `src/bindings/browser.rs`:

- `generate_pdf(input: JsValue) -> Result<Vec<u8>, JsError>`:
  1. `serde_wasm_bindgen::from_value(input)` → ExamSpec
  2. Monta RenderContext com thread-locals
  3. Chama `pipeline::render`
  4. Retorna bytes ou JsError com mensagem
- `set_font_rules(input: JsValue)`: deserializa FontRulesInput e atualiza thread-local
- Exportar TypeScript types via `wasm-bindgen` attributes

**Critério:** `generate_pdf(validSpec)` retorna `Uint8Array` com PDF válido.

### TASK-033 — Bindings WASI: C-ABI completo `[x]`
Atualizar `src/bindings/wasi.rs`:

- `prova_pdf_generate`: deserializa JSON, monta RenderContext, chama pipeline, copia bytes para buffer de saída
- `prova_pdf_set_font_rules`: deserializa JSON com FontRulesInput
- Todos os erros armazenados em `LAST_ERROR` thread-local
- Retorno convencional: `>= 0` → bytes escritos; `< 0` → código de erro

**Critério:** `cargo test --target wasm32-wasip1` gera PDF para fixture simples.

---

## Fase 8 — Matemática

### TASK-034 — Math rendering com pulldown-latex `[x]`
Implementar `src/math/` (feature "math"):

- `parser.rs`: converte string LaTeX → `MathExpr` AST
- `layout.rs`: `MathExpr → Vec<Fragment>`
  - Frações: numerador / denominador com linha entre eles
  - Raízes: símbolo √ + overline + conteúdo
  - Somatório/integral: símbolo grande + limites sup/inf como Sup/Sub
  - Matrizes: grid de Fragment
- Fonte Math: usa família "math" do FontRules; fallback para "body"
- Inline vs display: display centraliza e aumenta font_size × 1.5

**Critério:** `$\frac{a}{b}$` e `$$\sum_{i=0}^{n} x_i$$` renderizados corretamente.

---

## Fase 9 — Pacotes de distribuição

### TASK-035 — Pacote npm com TypeScript types `[x]`
Configurar `npm/` com wasm-bindgen output:

- `package.json`: nome `prova-pdf`, version, main/module/types
- Gerar `.d.ts` com `wasm-bindgen --typescript`
- Interfaces TypeScript para `ExamSpec`, `PrintConfig`, todas as enums
- Arquivo `index.js` com inicialização do WASM (`init()`)
- README com exemplo de uso em 10 linhas

**Critério:** `npm pack` funciona; TypeScript sem erros em exemplo de uso.

### TASK-036 — Pacote Python `[x]`
Implementar `packages/python/`:

- `prova_pdf/__init__.py`: wrapper sobre WASI via `wasmtime`
- `generate_pdf(spec: dict, fonts: list[FontInput]) -> bytes`
- `FontInput = { "family": str, "variant": int, "data": bytes }`
- `pyproject.toml` com metadata e dependência de wasmtime
- Bundlar `prova_pdf.wasm` dentro do pacote

**Critério:** `pip install -e .` + gerar PDF de fixture em Python.

### TASK-037 — Pacote Go `[x]`
Implementar `packages/go/`:

- `exampdf/exampdf.go`: wrapper sobre WASI via `wazero`
- `func GeneratePDF(spec []byte, fonts []FontInput) ([]byte, error)`
- `go.mod` com wazero dependency
- Bundlar `prova_pdf.wasm` com `//go:embed`

**Critério:** `go test ./...` gera PDF de fixture em Go.

---

## Fase 10 — Comparação visual por partes isoladas

### Contexto e motivação

O fluxo anterior tentava comparar PDFs completos (prova inteira) de uma vez, medindo SSIM
global por página. Isso misturava divergências de cabeçalho, questões, espaçamentos e
paginação num único score, dificultando a identificação e correção de problemas isolados.

**Nova estratégia:** comparar **cada parte da prova separadamente**, validando uma de cada
vez antes de compor o documento completo. Cada parte é renderizada isoladamente tanto pelo
Chromium (via Django) quanto pelo prova-pdf, e comparada visualmente.

**Sequência:**
1. **Cabeçalho institucional** (header) — primeira parte a validar
2. **Questão objetiva** (choice) — com variaç��es de layout
3. **Questão dissertativa** (textual) — linhas, blank, noBorder
4. **Questão de somatório** (sum)
5. **Questão cloze** (lacunas)
6. **Questão essay** (redação)
7. **Questão file** (upload)
8. **Seções** (título, instruções, separação por disciplina)
9. **Textos-base** (7 posições)
10. **Composição completa** — validação de espaçamento e paginação do PDF inteiro

O fluxo atual de geração de referência (Chromium) permanece:
- Template: `lizeedu/fiscallizeon/exams/templates/dashboard/exams/exam_print.html`
- pdf-service: `lize/pdf-service/` (Go, go-rod, pdfcpu)
- CSS: `exam-print.css` + inline styles no template

### TASK-038 — Fixtures de ExamSpec a partir de provas reais `[x]`
*(concluída — mantida para referência)*

### TASK-039 — Testes cross-platform: browser == WASI `[x]`
*(conclu��da — mantida para referência)*

### TASK-040 — Captura de PDFs de referência via Chromium `[x]`
*(concluída para provas completas — será estendida por parte nas tasks seguintes)*

---

### Etapa 10A — Cabeçalho institucional (header)

#### TASK-041 — Django: rota para renderizar apenas o header `[ ]`

Criar no Django (lizeedu) uma nova view que renderiza **apenas o cabeçalho institucional**
da prova, sem questões, sem seções, sem rodapé. O objetivo é gerar um PDF via Chromium
contendo somente o header, para comparação isolada com o prova-pdf.

**Implementação:**
1. Nova view: `ExamHeaderOnlyPrintView` em `exams/views/`
2. Nova URL: `provas/<pk>/imprimir-header/` (ou param `?render_part=header`)
3. Reutiliza **exatamente** a mesma lógica de renderização de cabeçalho do `exam_print.html`:
   - Logo (se configurado)
   - Nome da instituição/escola
   - Título da prova, disciplina, ano
   - Campos de aluno (Nome, Turma, Data, Nota — conforme `studentFields`)
   - Instruções
   - Linha separadora
4. O template pode ser um novo `exam_header_only.html` que herda do mesmo base
   e renderiza apenas o bloco de header, OU o template original com flag `{% if render_part == 'header' %}`
5. Deve respeitar **todas as mesmas regras** do template original:
   - CSS `@media print`, `@page` com margens corretas
   - `font-family`, `font-size` conforme `PrintConfig`
   - `allBlack` → `* { color: black !important }`
   - `headerFull` → exibir/ocultar campos de aluno
   - Layout de tabela com logo, bordas, espaçamentos

**Parametrização via query string** (mesmos params do `exam_print.html`):
- `paper_size=a4|ata`
- `font_size=0|1|2|3|4|5|6|7` (índice do FONT_SIZE_MAP)
- `font_family=ibm|verdana|times|arial`
- `all_black=0|1`
- `header_full=0|1`
- `margin_top=X&margin_bottom=X&margin_left=X&margin_right=X`

**Critério:** a rota renderiza apenas o header; o resultado visual é **idêntico** ao header
que aparece no topo da página 1 do `exam_print.html` completo.

#### TASK-042 — Captura de PDFs de referência: headers isolados `[ ]`

Gerar PDFs de referência do header via Chromium para múltiplas variações de parâmetros.
Cada variação exercita uma combinação diferente de config.

**Variações obrigatórias:**

| Caso | Parâmetros | O que exercita |
|------|-----------|----------------|
| `header_a4_default` | A4, 1col, font_size=12, headerFull=true | Caso base |
| `header_a4_nofull` | A4, headerFull=false | Sem campos de aluno |
| `header_ata` | ATA 200×266mm | Página menor |
| `header_allblack` | allBlack=true | Cores forçadas para preto |
| `header_font_large` | fontSize=18pt | Fonte grande |
| `header_font_small` | fontSize=8pt | Fonte pequena |
| `header_with_logo` | Logo configurado | Logo à esquerda |
| `header_custom_margins` | margins diferentes do padrão | Margens afetam largura |
| `header_verdana` | fontFamily=verdana | Família de fonte diferente |
| `header_instructions` | Instruções com formatação (bold, math) | Bloco de instruções |

**Processo para cada variação:**
1. Chamar `POST /print` do pdf-service com a URL da nova rota `imprimir-header/`
2. Salvar PDF em `tests/visual/reference/chromium/parts/header/<caso>.pdf`
3. Converter para PNG a 150 DPI
4. Registrar no manifest

**Critério:** 10 PDFs de header capturados cobrindo todas as variações de config relevantes.

#### TASK-043 — Parser Python: gerar fixtures apenas com header `[ ]`

Estender o parser Python (`generate_case_specs.py` ou novo script `generate_part_specs.py`)
para gerar ExamSpec JSON contendo **apenas o header** (sem sections/questions).

**Output:**
```json
{
  "_part": "header",
  "_case": "header_a4_default",
  "_params": "paper_size=a4&header_full=1",
  "config": { "pageSize": "A4", "fontSize": 12, "headerFull": true, ... },
  "header": {
    "institution": "...",
    "title": "...",
    "subject": "...",
    "year": "...",
    "studentFields": [...],
    "instructions": [...]
  },
  "sections": []
}
```

- Gerar uma fixture por variação da TASK-042 (mesmos 10 casos)
- Cada fixture deve refletir **exatamente** os mesmos dados usados na captura Chromium
- Salvar em `tests/fixtures/parts/header/<caso>.json`

**Critério:** 10 fixtures de header, uma por variação, dados idênticos à referência Chromium.

#### TASK-044 — Comparação visual: header isolado `[ ]`

Adaptar o `compare.py` (ou criar `compare_parts.py`) para comparar partes isoladas.

**Fluxo:**
1. Para cada caso de header:
   - Gerar PDF via Go wrapper com a fixture header-only
   - Converter para PNG a 150 DPI
   - Comparar com a referência Chromium (SSIM + diff visual)
2. Gerar relatório HTML por caso
3. Registrar em `tests/visual/CALIBRATION_PARTS.md`

**Threshold:** SSIM ≥ 0.90 para header isolado (sem ruído de questões/paginação).

**Critério:** todos os 10 casos de header com SSIM ≥ 0.90 ou divergências documentadas.

#### TASK-045 — Calibração do header `[ ]`

Ajustar constantes de layout do header no prova-pdf (`src/layout/header.rs`) até atingir
paridade visual com o Chromium em todas as variações.

**Constantes a calibrar:**
- `LOGO_DEFAULT_HEIGHT_CM`, `LOGO_CELL_PAD_PT`
- `BODY_FONT_SIZE_PT` (tamanho do texto institucional)
- Espaçamento entre linhas do header (institution, title, subject)
- Largura/espaçamento dos campos de aluno (`StudentField`)
- Margem antes/depois das instruções (`INSTRUCTIONS_TOP_MARGIN_PT`)
- Espessura e posição da linha separadora

**Processo:**
1. Comparar header_a4_default → identificar primeiras divergências
2. Ajustar constante → re-gerar → re-comparar
3. Validar que o ajuste não quebra as outras variações
4. Documentar cada rodada em `CALIBRATION_PARTS.md`

**Critério:** SSIM ≥ 0.90 para todos os 10 casos de header.

---

### Etapa 10B — Questões isoladas (por tipo)

#### TASK-046 — Django: rota para renderizar uma questão isolada `[ ]`

Criar view que renderiza **uma única questão** (sem header, sem seção), para cada tipo:

**URL:** `provas/<pk>/imprimir-questao/<question_id>/`

**Query params:** mesmos do `exam_print.html` + `kind=choice|textual|sum|cloze|essay|file`

A view deve renderizar a questão com o **mesmo CSS e lógica** do template original:
- Número da questão, pontuação (se `showScore`)
- Stem (enunciado) com formatação inline
- Espaço de resposta conforme o tipo
- Draft lines (se configurado)
- BaseTexts (se existirem na questão)

**Variações por tipo de questão:**

| Tipo | Variações a testar |
|------|-------------------|
| `choice` | vertical vs horizontal; 3/4/5 alternativas; com imagem no stem; com math |
| `textual` | lines (3/5/8 linhas); blank; noBorder; lineHeightCm variado |
| `sum` | 4/8 itens; showSumBox true/false |
| `cloze` | com/sem wordBank; blanks de larguras diferentes |
| `essay` | lineCount=15/30; heightCm fixo; fullWidth |
| `file` | label padrão; label customizado |

**Critério:** rota funcional para cada tipo; visual idêntico à questão no template completo.

#### TASK-047 — Captura + fixtures + comparação: questões isoladas `[ ]`

Para cada tipo de questão e suas variações:
1. Capturar PDF de referência via Chromium (rota da TASK-046)
2. Gerar fixture ExamSpec com `sections: [{ questions: [<questão>] }]`
3. Gerar PDF via prova-pdf
4. Comparar SSIM
5. Calibrar constantes específicas do tipo

**Estrutura de saída:**
```
tests/visual/reference/chromium/parts/question/
  ├── choice_vertical_5alt.pdf
  ├── choice_horizontal_3alt.pdf
  ├── textual_lines_5.pdf
  ├── textual_blank.pdf
  ├── sum_4items_sumbox.pdf
  ���── cloze_with_wordbank.pdf
  ├── essay_30lines.pdf
  └── file_default.pdf
tests/fixtures/parts/question/
  ├─�� choice_vertical_5alt.json
  ├── ...
```

**Threshold:** SSIM ≥ 0.90 por questão isolada.

**Critério:** todos os tipos de questão com todas as variações atingem SSIM ��� 0.90.

---

### Etapa 10C — Seções e textos-base

#### TASK-048 — Django: rota para renderizar seção isolada `[ ]`

View que renderiza **uma seção** (título + instruções + N questões simples), sem header.

**Variações:**
- Seção com título + 3 questões choice
- Seção com category badge
- Seção com `forcePageBreak`
- Seção com instruções formatadas (bold, italic)

#### TASK-049 — Django: rota para renderizar texto-base isolado `[ ]`

View que renderiza **uma questão com texto-base** em cada posição:
- `beforeQuestion`, `afterQuestion`
- `leftOfQuestion`, `rightOfQuestion`
- `sectionTop` (precisa de seção)

**Variações:** com/sem título, com/sem attribution, com imagem.

#### TASK-050 — Captura + fixtures + comparação: seções e textos-base `[ ]`

Mesmo fluxo das etapas anteriores:
1. Capturar referência Chromium
2. Gerar fixtures
3. Comparar SSIM
4. Calibrar

**Threshold:** SSIM ��� 0.88 (textos-base laterais podem ter mais variação).

**Critério:** todas as posições de texto-base e variações de seção validadas.

---

### Etapa 10D — Composição completa (espaçamento e paginação)

#### TASK-051 — Validação de espaçamento do PDF completo `[ ]`

Após validar todas as partes isoladamente, comparar o **PDF completo** gerado por ambos
os lados. Nesta etapa, as partes individuais já estão calibradas — o foco é exclusivamente
em **espaçamento entre elementos** e **paginação**.

**O que comparar:**
- Espaçamento entre questões (`question_spacing`)
- Espaçamento entre seções
- Espaçamento entre header e primeira questão
- Espaçamento entre stem e espaço de resposta
- Espaçamento entre alternativas
- Margem antes/depois de textos-base
- Altura total de cada questão (soma das partes)

**Casos de teste:** reutilizar os 10 casos completos já capturados (TASK-040):
- choice_a4_1col, choice_a4_2col, choice_ata_2col
- economy_allblack, textual_lines, sum_with_cloze
- full_header, break_all, font_size_large, multi_section

**Processo:**
1. Gerar PDF completo via prova-pdf para cada caso
2. Comparar número de páginas (deve ser idêntico — partes já calibradas)
3. Comparar SSIM por página
4. Se houver divergência de paginação: ajustar espaçamentos globais
5. Documentar em `CALIBRATION_SPACING.md`

**Threshold:** SSIM ≥ 0.85 por página; número de páginas idêntico.

**Critério:** 10 PDFs completos com número de páginas idêntico e SSIM médio ≥ 0.85.

#### TASK-052 — Validação de flags globais no PDF completo `[ ]`

Validar flags que afetam o documento inteiro, usando os PDFs completos:

| Flag | Validação |
|------|-----------|
| `economyMode` | 2 colunas forçadas, espaçamentos reduzidos, sem espaço de resposta |
| `allBlack` | Nenhuma cor diferente de preto |
| `breakAllQuestions` | Cada questão em página separada, contagem de páginas correta |
| `columns=2` | Layout bicolunado, linha divisória, balanceamento |
| `fullWidth` em questão | Questão full-width dentro de layout 2 colunas |
| `imageGrayscale` | Imagens em escala de cinza |

**Critério:** cada flag validada com SSIM ≥ 0.85 comparado à referência Chromium.

---

### Etapa 10E — Integração com pdf-service e Django

#### TASK-053 �� Integração do pdf-service: endpoint `/print-json` `[ ]`

Adicionar ao pdf-service da lize um novo endpoint que aceita ExamSpec JSON
e chama o prova-pdf (via Go wrapper wazero) em vez do Chromium.

```go
// pdf-service/print_json.go
router.POST("/print-json", printJsonPdf)

func printJsonPdf(c *gin.Context) {
    // 1. Parse ExamSpec JSON do body
    // 2. Carregar fontes de /fonts/ no container
    // 3. Chamar provapdf.GeneratePDF(spec, fonts)
    // 4. Pós-processar com pdfcpu (watermark, page numbers, blank pages)
    // 5. Retornar PDF ou upload S3
}
```

**Importante:** o endpoint `/print` (Chromium) continua funcionando.
O novo `/print-json` é uma rota paralela para testes A/B.

**Pré-requisito:** Etapas 10A–10D concluídas (todas as partes calibradas).

**Critério:** PDF gerado por `/print-json` visualmente comparável ao `/print`.

#### TASK-054 — Serialização Django: `exam_to_spec()` `[ ]`

Implementar no Django (lizeedu) a função que converte os modelos ORM em ExamSpec JSON.

Reutiliza o `exam_formatter.py` (já separado de acesso ao banco):
- `build_print_config(exam_row)` → PrintConfig
- `build_question(q, alts, base_texts, ...)` → Question
- `html_to_inline(html_str, images)` → InlineContent[]

```python
# exams/services/exam_spec_serializer.py
def exam_to_spec(exam: Exam, print_config: ExamPrintConfig) -> dict:
    """Serializa um Exam + PrintConfig do Django para ExamSpec JSON."""
```

**Critério:** `exam_to_spec(exam)` → JSON que passa na validação do prova-pdf.

#### TASK-055 — Testes A/B: Chromium vs prova-pdf em produção `[ ]`

Com ambos os endpoints funcionando (`/print` e `/print-json`):

1. Feature flag no Django para selecionar qual endpoint usar
2. Para X% dos pedidos, chamar ambos e comparar:
   - Tamanho do PDF (devem ser similares)
   - Tempo de geração (prova-pdf deve ser 20-100x mais rápido)
   - Número de páginas (deve ser idêntico)
3. Log de divergências para investigação
4. Rollout gradual: 1% → 10% → 50% → 100%

**Critério:** dashboard de métricas comparativas; zero divergência de número de páginas.

---

### Dependências entre tasks da Fase 10

```
Etapa 10A — Header isolado
  TASK-041 (Django: rota header-only)
      └─► TASK-042 (captura referências header)
              └─► TASK-043 (parser: fixtures header-only)
                      └─► TASK-044 (comparação SSIM header)
                              └─► TASK-045 (calibração header)

Etapa 10B �� Questões isoladas (após 10A concluída)
  TASK-046 (Django: rota questão isolada)
      └─► TASK-047 (captura + fixtures + comparação + calibração por tipo)

Etapa 10C — Seções e textos-base (após 10B concluída)
  TASK-048 (Django: rota seção isolada)
  TASK-049 (Django: rota texto-base isolado)
      └─► TASK-050 (captura + fixtures + comparação + calibração)

Etapa 10D — Composição completa (após 10A+10B+10C conclu��das)
  TASK-051 (validação espaçamento PDF completo)
  TASK-052 (validação flags globais)

Etapa 10E — Integração (após 10D concluída)
  TASK-053 (endpoint /print-json)
  TASK-054 (serialização Django)
      └─► TASK-055 (testes A/B em produção)
```

**Princípio:** cada etapa só inicia após a anterior estar com SSIM dentro do threshold.
Isso garante que ao compor o documento completo, as divergências restantes são apenas
de espaçamento global — não de renderização de partes individuais.

---

## Fase 11 — PrintConfig completo

### TASK-056 — economyMode, allBlack, breakAllQuestions `[ ]`
Implementar flags de config no pipeline:

- `economy_mode: true` → reduz `line_height × 0.7`, `margin × 0.85`, `blank_height × 0.7`
- `all_black: true` → força color=(0,0,0) em toda a cascata (implementar em TASK-010)
- `break_all_questions: true` → `new_page()` antes de cada questão no PageComposer
- `show_question_numbers: false` → ignora `Question.show_number`

### TASK-057 — Configuração de alternativas e questões `[ ]`
Implementar no renderer de questões:

- `alternative_spacing_cm` → espaçamento entre alternativas (Choice)
- `question_spacing_cm` → espaçamento entre questões
- `question_number_prefix` → "Q", "Questão", número limpo, etc.
- `columns_between_questions: bool` → se false, questões sempre em coluna única

### TASK-058 — Numeração e categorias de seção `[ ]`
Implementar tipos de numeração:

- `QuestionNumberingType::Global` → número sequencial do início ao fim (padrão)
- `QuestionNumberingType::PerSection` → reinicia a cada seção
- `QuestionNumberingType::None` → sem número
- `Section.category` → exibe badge no cabeçalho da seção

---

## Fase 12 — CI/CD e finalização

### TASK-059 — CI GitHub Actions (build e testes unitários) `[ ]`
Criar `.github/workflows/ci.yml`:

- Jobs: `test` (cargo test), `build-browser` (wasm-target), `build-wasi` (wasm-target)
- `size` job: publica tamanho do WASM como artefato e comenta em PRs
- Dependências de toolchain: `wasm32-unknown-unknown`, `wasm32-wasip1`, `wasm-bindgen-cli`, `wasm-opt`
- Cache de `target/` e `~/.cargo/registry`

### TASK-060 — Benchmarks de performance `[ ]`
Implementar `benches/`:

- `criterion` benchmark para fixture `simple_choice.json` (10 questões)
- `criterion` benchmark para fixture `all_kinds.json` (6 tipos)
- Target: < 200ms para 50 questões com LaTeX em wasm32-wasip1
- Medir separado: layout time, emission time, total time

### TASK-061 — Documentação da API pública `[ ]`
Escrever `README.md` com:

- Quickstart browser (5 linhas JS)
- Quickstart Python (5 linhas)
- Quickstart Go (5 linhas)
- Link para PROJECT.md (schema completo) e ARCHITECTURE.md (internals)
- Seção "Migração do Chromium" com link para MIGRATION.md no webassembly-pdf

---

## Resumo por fase

| Fase | Tasks | Descrição |
|------|-------|-----------|
| 0 | 001–006 | Scaffold, spec, fontes, bindings stub |
| 1 | 007–009 | Geometria, PageComposer, InlineLayout |
| 2 | 010–011 | Cascata de estilo, validação |
| 3 | 012–013 | Header institucional, running header |
| 4 | 014–021 | Renderização dos 6 tipos de questão + BaseText |
| 5 | 022–025 | full_width, draft_lines, appendix, seções |
| 6 | 026–030 | Emissão PDF, fontes, formas, imagens |
| 7 | 031–033 | Pipeline completo, bindings finais |
| 8 | 034 | Math LaTeX |
| 9 | 035–037 | npm, Python, Go |
| 10 | 038–055 | **Comparação visual por partes isoladas** (10A: header, 10B: questões, 10C: seções/textos-base, 10D: composição completa, 10E: integração lize) |
| 11 | 056–058 | PrintConfig completo |
| 12 | 059–061 | CI, benchmarks, docs |

**Pré-requisitos externos da Fase 10:**
- TASK-041/046/048/049: Acesso ao repositório `lize/lizeedu` para criar novas rotas Django
- TASK-042: Django + pdf-service + PostgreSQL rodando localmente
- TASK-053: Acesso ao repositório `lize/pdf-service` para adicionar endpoint
