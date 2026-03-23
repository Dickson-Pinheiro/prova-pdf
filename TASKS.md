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

## Fase 10 — Comparação visual e calibração contra o pdf-service da lize

### Contexto do fluxo atual

O pdf-service da lize gera PDFs assim:
1. Django renderiza `exam_print.html` (template Django + Vue.js + MathJax) com dados do banco
2. pdf-service (Go + go-rod) abre a URL num Chromium headless
3. Chromium renderiza HTML/CSS/JS → `page.PDF()` gera o PDF
4. pdfcpu pós-processa: watermark (footer), numeração de página, páginas em branco

Para comparar o prova-pdf contra esse fluxo, é necessário:
- Gerar o PDF pelo fluxo Chromium (referência)
- Gerar o PDF pelo prova-pdf a partir dos **mesmos dados do banco**
- Comparar visualmente

Locais relevantes da lize:
- Template: `lizeedu/fiscallizeon/exams/templates/dashboard/exams/exam_print.html`
- pdf-service: `lize/pdf-service/` (Go, go-rod, pdfcpu)
- CSS: `exam-print.css` + inline styles no template
- Dados: Vue `exam_data.js` alimenta as questões via API

### TASK-038 — Fixtures de ExamSpec a partir de provas reais `[~]`

Expandir o `parse_exam.py` (que já separa banco da formatação via `exam_formatter.py`)
para gerar fixtures representativas a partir de provas reais do banco da lize.

Fixtures já existentes (geradas pelo parse_exam.py):
- `all_kinds.json` ✅
- `simple_choice.json` ✅ (sintética)
- `portugues_poema.json` ✅
- `pga_2em_2trimestre.json` ✅
- `exatas.json` ✅
- `matematica_vml.json` ✅
- `p4_lingua_portuguesa.json` ✅

Fixtures adicionais necessárias (cobrir variações de PrintConfig):
- [ ] Prova com `economyMode=true` + `allBlack=true`
- [ ] Prova com `pageSize=Ata` (200×266mm) + 2 colunas
- [ ] Prova com `breakAllQuestions=true`
- [ ] Prova com header customizado (logo + campos de aluno completos)
- [ ] Prova com math LaTeX pesado (fórmulas display + inline)

**Critério:** 10+ fixtures cobrindo todas as variações de PrintConfig usadas em produção.

### TASK-039 — Testes cross-platform: browser == WASI `[x]`
Implementar `tests/cross-platform/`:

- Para cada fixture: gerar PDF via browser (wasm-bindgen + Node) e via WASI (Go)
- Comparar SHA-256 dos bytes → devem ser idênticos
- Script `run.sh` orquestra: build browser, build wasi, run Node, run Go binary, diff

**Critério:** todos os fixtures geram bytes idênticos nos 3 ambientes.

### TASK-040 — Captura de PDFs de referência via Chromium `[ ]`

Gerar PDFs de referência chamando o fluxo real da lize (Django + pdf-service + Chromium).

**Pré-requisitos:** ambiente local da lize rodando (Django + pdf-service + PostgreSQL).

**Estratégia:**
1. Selecionar 10 provas no banco que cubram as variações de layout (ver lista abaixo)
2. Para cada prova, chamar `POST /print` do pdf-service com a URL do `exam_print.html`
3. Salvar o PDF de referência em `tests/visual/reference/chromium/<case_name>.pdf`
4. Converter para PNG (300 DPI) com `pdftoppm` para comparação pixel-level
5. Em paralelo, gerar o ExamSpec JSON da mesma prova via `parse_exam.py`
6. Registrar em `tests/visual/manifest.json`: exam_id, parâmetros usados, data

**Casos obrigatórios:** (cada um exercita flags diferentes do template)

| Caso | PrintConfig exercitado | Regra CSS/template correspondente |
|------|------------------------|-----------------------------------|
| `choice_a4_1col` | A4, 1 coluna, IBM Plex Sans | `@page { size: a4 }`, `column-count: unset` |
| `choice_a4_2col` | A4, 2 colunas | `column-count: 2`, `column-rule: 1px solid #3b4863` |
| `choice_ata_2col` | ATA 200×266mm, 2 colunas | `@page { size: 200mm 266mm }` |
| `economy_allblack` | economyMode + allBlack | `* { color: black !important }`, 2 colunas forçadas |
| `textual_lines` | Questões dissertativas com linhas | `.discursive-line-height`, `border-bottom border-dark` |
| `sum_with_cloze` | Somatório + cloze | Alternativas com `Math.pow(2, index)`, lacunas |
| `full_header` | Header com logo, campos, instruções | Tabela `table-bordered` com logo + campos aluno |
| `break_all` | breakAllQuestions=true | `.pagebreak { break-before: right }` |
| `font_size_large` | fontSize=18pt | `.question * { font-size: 18pt !important }` |
| `multi_section` | 3 seções separadas por disciplina | `separate_subjects == 1`, `subject-section` page-break |

**Critério:** 10 PDFs de referência capturados + ExamSpec equivalente validado.

### TASK-041 — Mapeamento CSS↔Layout: documentar regras de correspondência `[ ]`

Documentar como cada regra CSS/template do `exam_print.html` corresponde a parâmetros
do motor de layout do prova-pdf. Este mapeamento é a base para calibração.

**Correspondências a documentar:**

| Elemento | CSS da lize (exam_print.html) | prova-pdf (Rust) |
|----------|-------------------------------|-------------------|
| Fonte base | `font-size: .875rem` (=14px screen, `font_size`pt no @media print) | `PrintConfig.font_size` |
| Line-height | `line-height: 1.5` / `2.5` / `3.5` / `normal` | `LineSpacing` enum multiplier |
| Margem página | `@page { margin-left/top/right/bottom }` em cm | `Margins { top, bottom, left, right }` em cm |
| 2 colunas | `column-count: 2; column-gap: 35px; column-rule: 1px solid #3b4863` | `columns: 2`, `column_gap_pt: 14.0`, VRule 0.75pt |
| Número questão | `span.question-number` (circle badge, 29px, bg-black) | `format_number()` + GlyphRun badge |
| Alternativa badge | `span.question-alternative` (circle, bold, uppercase) | `render_choice()` com `alt_badge_scale` |
| Linha resposta | `hr.border-dark` com `margin-top: Xcm` | `HRule` com `discursive_line_height` cm |
| Rascunho | `.draft-area { border: 2px solid black }` + `.draft-row { min-height: 7mm }` | `draft_lines` × `draft_line_height` |
| Page break | `.pagebreak { break-before: right / break-after: page }` | `force_page_break` / `break_all_questions` |
| Full-width | `.force-one-column { column-span: all }` | `full_width: true` |
| allBlack | `* { color: black !important }` | `apply_all_black()` pós-processamento |
| Espaçamento MathJax | `mjx-container.MathJax { font-size: 120% !important }` | MathLayout scale factor |

**Critério:** tabela completa em `tests/visual/CSS_LAYOUT_MAP.md` com todas as correspondências.

### TASK-042 — Comparação visual: SSIM por página `[ ]`

Implementar `tests/visual/compare.py`:

```
python compare.py --case choice_a4_1col
```

1. Converter ambos os PDFs (referência Chromium + prova-pdf) para PNG a 300 DPI
2. Calcular SSIM por página
3. Gerar relatório HTML com imagens lado a lado + diff colorido + scores
4. Threshold geral: SSIM ≥ 0.85 (tolerância maior porque são engines diferentes)

O objetivo não é paridade pixel-perfect (impossível entre Chromium e um motor Rust),
mas garantir que a **estrutura visual** é equivalente: mesmos elementos na mesma posição,
mesma hierarquia, mesmo número de páginas.

**Critério:** script funciona para os 10 casos; relatório HTML gerado.

### TASK-043 — Calibração iterativa de constantes de layout `[ ]`

Ajustar as constantes do prova-pdf para maximizar semelhança visual com o Chromium.

**Processo iterativo:**
1. Executar `compare.py` para todos os casos → baseline de scores
2. Identificar as maiores divergências (espaçamentos, tamanhos, margens)
3. Comparar com os valores CSS reais do template:
   - `column-gap: 35px` (CSS) → ajustar `column_gap_pt` (Rust)
   - `font-size: .875rem` / `15pt` → ajustar `font_size` default
   - `margin-top: 0.85cm` para linhas → ajustar `discursive_line_height`
   - `min-height: 7mm` para draft → ajustar `draft_line_height`
   - `width: 29px` para badge → ajustar `alt_badge_scale`
4. Re-executar compare.py → documentar delta de SSIM
5. Repetir até convergir

**Fontes-chave:** O CSS da lize usa `IBM Plex Sans` (Google Fonts) + `Noto Sans Math`.
O prova-pdf precisa receber os mesmos TTFs para comparação justa.

**Critério:** `tests/visual/CALIBRATION.md` com pelo menos 3 rodadas documentadas.

### TASK-044 — Integração do pdf-service: endpoint `/print-json` `[ ]`

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

**Pré-requisito:** TASK-043 concluída (constantes calibradas).

**Critério:** PDF gerado por `/print-json` com mesma prova do banco → visualmente comparável ao `/print`.

### TASK-044b — Serialização Django: `exam_to_spec()` `[ ]`

Implementar no Django (lizeedu) a função que converte os modelos ORM em ExamSpec JSON.

Reutiliza o `exam_formatter.py` (já separado de acesso ao banco):
- `build_print_config(exam_row)` → PrintConfig
- `build_question(q, alts, base_texts, ...)` → Question
- `html_to_inline(html_str, images)` → InlineContent[]

**Novo no Django:**
```python
# exams/services/exam_spec_serializer.py
def exam_to_spec(exam: Exam, print_config: ExamPrintConfig) -> dict:
    """Serializa um Exam + PrintConfig do Django para ExamSpec JSON."""
    # Usa exam_formatter.build_print_config, build_question, etc.
```

**Critério:** `exam_to_spec(exam)` → JSON que passa na validação do prova-pdf.

### TASK-044c — Testes A/B: Chromium vs prova-pdf em produção `[ ]`

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

## Fase 11 — PrintConfig completo

### TASK-045 — economyMode, allBlack, breakAllQuestions `[ ]`
Implementar flags de config no pipeline:

- `economy_mode: true` → reduz `line_height × 0.7`, `margin × 0.85`, `blank_height × 0.7`
- `all_black: true` → força color=(0,0,0) em toda a cascata (implementar em TASK-010)
- `break_all_questions: true` → `new_page()` antes de cada questão no PageComposer
- `show_question_numbers: false` → ignora `Question.show_number`

### TASK-046 — Configuração de alternativas e questões `[ ]`
Implementar no renderer de questões:

- `alternative_spacing_cm` → espaçamento entre alternativas (Choice)
- `question_spacing_cm` → espaçamento entre questões
- `question_number_prefix` → "Q", "Questão", número limpo, etc.
- `columns_between_questions: bool` → se false, questões sempre em coluna única

### TASK-047 — Numeração e categorias de seção `[ ]`
Implementar tipos de numeração:

- `QuestionNumberingType::Global` → número sequencial do início ao fim (padrão)
- `QuestionNumberingType::PerSection` → reinicia a cada seção
- `QuestionNumberingType::None` → sem número
- `Section.category` → exibe badge no cabeçalho da seção

---

## Fase 12 — CI/CD e finalização

### TASK-048 — CI GitHub Actions (build e testes unitários) `[ ]`
Criar `.github/workflows/ci.yml`:

- Jobs: `test` (cargo test), `build-browser` (wasm-target), `build-wasi` (wasm-target)
- `size` job: publica tamanho do WASM como artefato e comenta em PRs
- Dependências de toolchain: `wasm32-unknown-unknown`, `wasm32-wasip1`, `wasm-bindgen-cli`, `wasm-opt`
- Cache de `target/` e `~/.cargo/registry`

### TASK-049 — Benchmarks de performance `[ ]`
Implementar `benches/`:

- `criterion` benchmark para fixture `simple_choice.json` (10 questões)
- `criterion` benchmark para fixture `all_kinds.json` (6 tipos)
- Target: < 200ms para 50 questões com LaTeX em wasm32-wasip1
- Medir separado: layout time, emission time, total time

### TASK-050 — Documentação da API pública `[ ]`
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
| 10 | 038–044c | **Comparação visual e integração lize** (fixtures reais, mapeamento CSS↔Layout, SSIM, calibração, endpoint /print-json, serialização Django, testes A/B) |
| 11 | 045–047 | PrintConfig completo |
| 12 | 048–050 | CI, benchmarks, docs |

---

## Dependências entre tasks da Fase 10

```
TASK-038 (fixtures reais via parse_exam.py)
    │
    ├─► TASK-040 (captura PDFs referência Chromium) ◄── requer Django + pdf-service rodando
    │       │
    │       ├─► TASK-041 (mapeamento CSS ↔ Layout)
    │       │
    │       └─► TASK-042 (compare.py SSIM por página)
    │               │
    │               └─► TASK-043 (calibração iterativa de constantes)
    │
    └─► TASK-044 (endpoint /print-json no pdf-service)
            │
            ├── TASK-044b (serialização Django: exam_to_spec)
            │
            └─► TASK-044c (testes A/B: Chromium vs prova-pdf)
```

**Fluxo:** fixtures reais (038) alimentam tanto a captura de referência (040) quanto
a integração direta no pdf-service (044). A calibração (043) é iterativa e ocorre em
paralelo com o refinamento do layout. O rollout em produção (044c) é o passo final.

**Pré-requisitos externos:**
- TASK-040: Django + pdf-service + PostgreSQL rodando localmente
- TASK-044: Acesso ao repositório `lize/pdf-service` para adicionar endpoint
- TASK-044b: Acesso ao repositório `lize/lizeedu` para adicionar serializer
