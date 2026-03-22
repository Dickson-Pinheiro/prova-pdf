# prova-pdf — Architecture

## 1. Princípios

| Princípio | Decisão |
|-----------|---------|
| **Zero dependências de sistema** | Compilado para WASM; sem libc, sem SO, sem Chromium |
| **Schema domain-first** | `ExamSpec` fala o idioma da prova, não de um renderizador genérico |
| **Fontes externas e configuráveis** | `FontRegistry` + `FontRules` — sem fonte embutida no binário |
| **Precisão visual reproduzível** | Mesmo algoritmo de layout para browser, Python e Go → byte-to-byte igual |
| **Binário enxuto** | `opt-level='z'`, LTO, `strip`, `panic=abort`, wasm-opt `-Oz` |
| **Sem alocador personalizado** | `wee_alloc` descartado; dlmalloc padrão do WASI é suficiente e mais seguro |

---

## 2. Módulos

```
src/
├── spec/           ← Schema público (serialização JSON in)
│   ├── exam.rs         ExamSpec, Section, Appendix
│   ├── question.rs     Question, QuestionKind, BaseText, BaseTextPosition
│   ├── answer.rs       AnswerSpace + 6 variantes
│   ├── inline.rs       InlineContent (Text/Math/Image/Sub/Sup/Blank)
│   ├── header.rs       InstitutionalHeader, StudentField, RunningHeader
│   ├── config.rs       PrintConfig, PageSize, Margins, LineSpacing, …
│   └── style.rs        Style (cascading), FontWeight, FontStyle
│
├── fonts/          ← Registro e resolução de fontes
│   ├── data.rs         FontData (bytes + ttf-parser face), FontFamily
│   ├── registry.rs     FontRegistry (HashMap), FontRules
│   └── resolve.rs      FontResolver, FontRole, pick_variant()
│
├── layout/         ← Motor de layout → Fragment IR
│   ├── fragment.rs     Fragment, FragmentKind, GlyphRun, HRule, …
│   ├── inline.rs       InlineLayoutEngine — quebra de linha, shaping
│   ├── page.rs         PageComposer — empilhamento vertical, colunas, paginação
│   ├── header.rs       layout do InstitutionalHeader
│   ├── question.rs     layout de cada QuestionKind
│   ├── answer.rs       layout de cada AnswerSpace
│   └── base_text.rs    posicionamento de BaseText nas 7 posições
│
├── pdf/            ← Emissão PDF a partir do Fragment IR
│   ├── emit.rs         PdfEmitter — converte Vec<PageFragments> → Vec<u8>
│   ├── fonts.rs        subsetting (subsetter), embedding, ToUnicode CMap
│   ├── images.rs       embedding JPEG/PNG via miniz_oxide
│   └── drawing.rs      helpers: hrule, filled_rect, stroked_rect
│
├── math/           ← Renderização LaTeX (feature "math")
│   ├── parser.rs       pulldown-latex → MathExpr
│   └── layout.rs       MathExpr → Vec<Fragment>
│
├── color.rs        ← Parsing CSS color → (r, g, b) f32
├── pipeline.rs     ← Orquestra 4 fases; RenderContext; PipelineError
│
└── bindings/       ← Pontos de entrada WASM
    ├── browser.rs      wasm-bindgen (feature "browser")
    └── wasi.rs         WASI C-ABI prova_pdf_* (feature "wasi-lib")
```

---

## 3. Pipeline de 4 fases

```
ExamSpec (JSON)
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ Fase 1 — Validação                                      │
│  • campos obrigatórios presentes                        │
│  • registry.is_ready() — pelo menos 1 família com body  │
│  • alternativas únicas dentro de cada questão           │
│  • image_key existe no ImageStore quando referenciado   │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│ Fase 2 — Cascata de Estilo                              │
│  PrintConfig → Section.style → Question → Inline        │
│  Produz ResolvedStyle por elemento                      │
│  FontResolver: override → FontRules[role] → body → 1st  │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│ Fase 3 — Layout                                         │
│  PageComposer empilha elementos verticalmente           │
│  InlineLayoutEngine: shaping (rustybuzz) + line-break   │
│  Produz Vec<Page> onde Page = Vec<Fragment>             │
│  Paginação automática; force_page_break; full_width     │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│ Fase 4 — Emissão PDF                                    │
│  PdfEmitter: pdf-writer 0.14                            │
│  • Subsets de fontes por página (subsetter 0.2)         │
│  • CMap ToUnicode para copy-paste                       │
│  • Imagens JPEG/PNG comprimidas (miniz_oxide)           │
│  • content stream por página                            │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
                   Vec<u8> (PDF)
```

---

## 4. Fragment IR

O Fragment IR é a representação intermediária entre o layout e a emissão.
Cada `Fragment` é uma unidade atômica posicionada em coordenadas de página (pontos PDF, origem no canto superior esquerdo da área de conteúdo).

```rust
struct Fragment {
    x:      f64,   // pts, da borda esquerda da área de conteúdo
    y:      f64,   // pts, do topo da área de conteúdo (cresce ↓)
    width:  f64,
    height: f64,
    kind:   FragmentKind,
}

enum FragmentKind {
    GlyphRun(GlyphRun),      // texto já shaped
    HRule(HRule),            // linha horizontal
    FilledRect(FilledRect),  // retângulo preenchido
    StrokedRect(StrokedRect),// retângulo com borda
    Image(ImageFragment),    // imagem raster
    Spacer,                  // espaço vertical sem conteúdo
}
```

**Por que Fragment IR?**
- Separa o "o quê" do "como": o layout não conhece PDF; o emitidor não conhece semântica.
- Permite testar o layout sem gerar PDF.
- Facilita futuramente adicionar SVG ou canvas como targets alternativos.

---

## 5. Sistema de Fontes

### 5.1 Estruturas

```
FontRegistry
  └── HashMap<String, FontFamily>
        FontFamily
          ├── regular: FontData          (obrigatório)
          ├── bold: Option<FontData>
          ├── italic: Option<FontData>
          └── bold_italic: Option<FontData>

FontData
  ├── bytes: Vec<u8>             (TTF/OTF raw)
  └── face: OwnedFace            (ttf-parser, zero-copy sobre bytes)

FontRules
  ├── body:     String  ("body")
  ├── heading:  String  ("body")
  ├── question: String  ("body")
  └── math:     String  ("body")
```

### 5.2 Resolução

```
resolve(role, weight, style, family_override?)
  1. family_override → registry.get(name)
  2. FontRules[role]  → registry.get(name)
  3. "body"           → registry.body()
  4. first registered → registry.family_names().next()
  5. panic (impossível em produção: validação na fase 1 garante is_ready())
```

### 5.3 Shaping e Subsetting

- **rustybuzz 0.20** — shaping HarfBuzz-compatível, puro Rust
- **subsetter 0.2** — extrai apenas os glifos usados por página → PDF menor
- Cada `GlyphRun` carrega `font_family + variant`; o emitidor agrupa por família para montar o subset

---

## 6. Motor de Layout Inline

`InlineLayoutEngine` opera sobre `Vec<InlineContent>` e produz `Vec<Fragment>`:

```
para cada InlineContent:
  Text  → shape com rustybuzz → GlyphRun(s) com x_advances
  Math  → MathLayout → Vec<Fragment>   (feature "math")
  Image → ImageFragment
  Sub   → recursivo com font_size * 0.65, baseline -0.35em
  Sup   → recursivo com font_size * 0.65, baseline +0.35em
  Blank → FilledRect (underline) com width_cm ou 3.5cm padrão

Quebra de linha:
  unicode-linebreak 0.1 determina oportunidades
  greedy-fill: empacota tokens até caber na largura disponível
  excesso → nova linha (y += line_height)
```

### Métricas de linha

```
line_height = font_size × line_spacing_factor
  Normal      = 1.4
  OneAndHalf  = 1.5
  TwoAndHalf  = 2.5
  ThreeAndHalf = 3.5

ascender/descender extraídos de ttf-parser::Face
baseline = y + ascender_pts
```

---

## 7. PageComposer

Responsável por empilhar elementos verticalmente e decidir quebras de página:

```
PageComposer {
    geometry:     PageGeometry,   // largura, altura, margens em pts
    cursor_y:     f64,            // posição vertical atual
    columns:      u8,             // 1 ou 2 (PrintConfig.columns)
    column_gap:   f64,
    current_page: Vec<Fragment>,
    pages:        Vec<Vec<Fragment>>,
}
```

**Algoritmo de paginação:**
1. Calcular a altura do próximo bloco
2. Se cursor_y + altura > page_height_pt − margin_bottom_pt → new_page()
3. `force_page_break: true` na Question → new_page() imediato
4. `break_all_questions: true` no PrintConfig → new_page() antes de cada questão

**Colunas:**
- 2 colunas: largura = (content_width − column_gap) / 2
- `full_width: true` na Question → ocupa as 2 colunas (column_span: all)
- Balanceamento: ao atingir metade do espaço vertical, avança para coluna 2

---

## 8. Renderização de Questões

Cada `QuestionKind` tem um renderer dedicado em `layout/question.rs`:

| Kind | Renderer | Elementos gerados |
|------|----------|-------------------|
| Choice | `render_choice()` | stem + lista de alternativas (bullet letter + texto) |
| Textual | `render_textual()` | stem + linhas de resposta (HRule × line_count) |
| Cloze | `render_cloze()` | stem com Blank inline + word_bank opcional |
| Sum | `render_sum()` | stem + itens com caixas de sub-resultado + caixa total |
| Essay | `render_essay()` | stem + área em branco (height_cm) ou linhas |
| File | `render_file()` | stem + label de instrução de upload |

**BaseText** (7 posições):
```
ExamTop          → topo do documento, antes do header
SectionTop       → topo de cada seção, antes do título
BeforeQuestion   → acima do stem, dentro do bloco da questão
AfterQuestion    → abaixo do espaço de resposta
LeftOfQuestion   → coluna lateral esquerda (full_width implícito)
RightOfQuestion  → coluna lateral direita (full_width implícito)
ExamBottom       → rodapé do documento, última página
```

---

## 9. InstitutionalHeader

```
┌────────────────────────────────────────────────────────┐
│  [logo]   INSTITUIÇÃO                                  │
│           Título da Prova                              │
│           Disciplina · Ano                             │
├────────────────────────────────────────────────────────┤
│  Nome: ________________________  Turma: __________     │
│  Matrícula: __________________  Data:  __________     │
├────────────────────────────────────────────────────────┤
│  Instruções: texto inline com formatação               │
└────────────────────────────────────────────────────────┘
```

- `StudentField[]` → campos dinâmicos com `width_cm` opcional
- `RunningHeader` → cabeçalho/rodapé de página (left/center/right), tokens `{page}` e `{pages}`
- Logo: chave no `ImageStore`

---

## 10. Targets WASM

### 10.1 Browser (npm / wasm-bindgen)

```
target: wasm32-unknown-unknown
feature: browser

API JS:
  add_font(family_name: string, variant: 0|1|2|3, data: Uint8Array): void
  set_font_rules(rules: FontRulesInput): void
  add_image(key: string, data: Uint8Array): void
  clear_all(): void
  generate_pdf(spec: ExamSpec): Uint8Array   // throws on error
```

Estado thread-local: `FONT_REGISTRY`, `FONT_RULES`, `IMAGE_STORE`

### 10.2 WASI / C-ABI (Python + Go)

```
target: wasm32-wasip1
feature: wasi-lib

Exports:
  prova_pdf_alloc(len: u32) -> *mut u8
  prova_pdf_free(ptr: *mut u8, len: u32)
  prova_pdf_add_font(family_ptr, family_len, variant, data_ptr, data_len) -> i32
  prova_pdf_set_font_rules(json_ptr, json_len) -> i32
  prova_pdf_add_image(key_ptr, key_len, data_ptr, data_len) -> i32
  prova_pdf_clear_all()
  prova_pdf_generate(json_ptr, json_len, out_ptr, out_len) -> i32
  prova_pdf_last_error_len() -> u32
  prova_pdf_last_error_message(buf_ptr, buf_len)
```

Convenção: retorno `i32 < 0` = erro; `>= 0` = bytes escritos ou OK.

---

## 11. Tratamento de Erros

```
PipelineError (thiserror)
  ├── NoFont                  — fase 1: registry não pronto
  ├── ValidationError(String) — fase 1: campo inválido
  ├── LayoutError(String)     — fase 3: impossível fazer layout
  └── EmissionError(String)   — fase 4: falha no pdf-writer

RegistryError
  ├── FamilyNotFound(String)
  ├── InvalidVariant(u8)
  └── ParseError(String)      — ttf-parser recusou os bytes
```

**Regras:**
- `panic!` só em testes e helpers internos (`#[cfg(test)]`)
- Código de produção propaga `Result`; `unwrap()` apenas em invariantes provados
- WASI: erros serializados como string na thread-local `LAST_ERROR`

---

## 12. Estratégia de Testes

| Camada | Tipo | Descrição |
|--------|------|-----------|
| `spec/` | unitário | roundtrip JSON serde para cada struct |
| `fonts/` | unitário | registry, resolução, fallback chain |
| `layout/` | unitário | métricas de linha, quebra de linha, paginação |
| `layout/` | snapshot | Fragment IR para fixtures de ExamSpec |
| `pdf/` | integração | gera PDF e valida estrutura com lopdf |
| `tests/` | cross-platform | mesmo JSON → mesmo PDF em browser/WASI |
| `tests/` | visual | SSIM ≥ 0.92 vs PDF gerado pelo Chromium |

**Fixtures de ExamSpec:**
- `fixtures/simple_choice.json` — 5 questões choice, 1 fonte
- `fixtures/all_kinds.json` — 1 questão de cada kind
- `fixtures/full_header.json` — header completo com logo e campos
- `fixtures/multi_font.json` — 3 famílias, FontRules customizadas
- `fixtures/economy_mode.json` — economyMode=true, allBlack=true

---

## 13. Convenções de Código

- Módulos `pub(crate)` por padrão; `pub` apenas na API de bindings
- Tipos de erro derivam `thiserror::Error`
- Sem `unwrap()` em código não-teste sem comentário `// SAFETY:` ou `// invariant:`
- Coordenadas sempre em `f64` pontos PDF (1 pt = 1/72 polegada)
- Conversão cm→pt: `cm * 28.3465`
- Cores internas como `(f32, f32, f32)` normalizado [0,1]; parsing em `color.rs`
- Imports organizados: std → crates externas → crate internas (clippy::pedantic)
