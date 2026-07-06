<!--
NOTA DE PRESERVAÇÃO (2026-07-06)
O harness executável de comparação (tests/compare/: capture_reference.py, pdf_snapshot.py,
snapshot_diff.py, run_case.sh, audit_template_fields.py, references/, specs/, out/) e as
fixtures de provas reais (portugues_poema, exatas, matematica_vml) foram REMOVIDOS do repo.
Este documento é mantido apenas como REGISTRO da estratégia de compatibilidade que estava
sendo aplicada (Chromium/lize ↔ prova-pdf), para referência futura caso a calibração visual
seja retomada. Os scripts de serialização tests/parse_exam.py e tests/exam_formatter.py
foram mantidos (base do futuro serializer Django `exam_to_spec()`).
Ver também: memória do projeto `project_compatibility_strategy.md`.
-->

# Estratégia de Comparação: Chromium → prova-pdf

Este documento descreve como funciona o pipeline de comparação entre o PDF gerado pelo Chromium (sistema lize atual) e o PDF gerado pelo prova-pdf (novo motor Rust/WASM).

---

## 1. Visão Geral do Pipeline

```
[lize Django]              [prova-pdf]
capture_reference.py  →   parse_exam.py + exam_formatter.py
       ↓                           ↓
 references/<case>.pdf       specs/<case>.json
       ↓                           ↓
  pdf_snapshot.py           run_case.sh → Go wrapper → candidate.pdf
       ↓                           ↓
 ref.snapshot.json        cand.snapshot.json
                ↓         ↓
            snapshot_diff.py
                ↓
          diff.json + diff.md
```

**Comando principal:**
```bash
tests/compare/run_case.sh portugues_poema
tests/compare/run_case.sh --all
```

---

## 2. Geração do PDF de Referência (Chromium)

### Script: `capture_reference.py`

Faz uma requisição HTTP ao Django local (`http://localhost:8000`) passando os parâmetros do exame via URL. O Django renderiza o template `exam_print.html` e envia para o `pdf-service` (Go + Chromium headless), que produz o PDF.

**Pré-requisitos:**
```bash
docker-compose up -d  # na raiz do projeto lizeedu
```

**Como executar:**
```bash
cd tests
python compare/capture_reference.py portugues_poema
python compare/capture_reference.py --all
```

Os parâmetros de URL são lidos automaticamente do campo `_url_params` no spec JSON correspondente (`specs/<case>.json`), que espelha a função Django `get_filters_to_print()`.

---

## 3. Geração do ExamSpec JSON

### Scripts: `parse_exam.py` + `exam_formatter.py`

`parse_exam.py` conecta ao banco PostgreSQL do lize, busca os dados do exame e delega toda a formatação para `exam_formatter.py` (que pode ser reutilizado no Django).

```bash
cd tests
python parse_exam.py portugues_poema          # gera fixtures/portugues_poema.json
python parse_exam.py --all                     # gera todos os presets
python parse_exam.py <uuid>                    # por UUID direto
python parse_exam.py <uuid> --header-only      # só o header
python parse_exam.py <uuid> --question <qid>   # uma questão específica
```

**Variáveis de ambiente:**
```bash
LIZE_DB_HOST=localhost
LIZE_DB_PORT=8888
LIZE_DB_NAME=lize_master_db
LIZE_DB_USER=postgres
LIZE_DB_PASSWORD=postgres
```

### Lógica de `exam_formatter.py` — regras críticas

#### Header (`build_header`)

Reproduz a lógica do template Django `not_separate.html` / `separate_subjects.html`:

| Condição | Campos mostrados |
|----------|-----------------|
| `header_format == 0` e `kind != 1` | Apenas ALUNO |
| `header_format == 1` ou `kind == 1` | ALUNO + Nº + SÉRIE + TURMA + TURNO + PROFESSOR |

- `header_format` → URL param `header_full`
- `kind` → URL param `separate_subjects` (1 = per-subject layout, que sempre mostra campos completos)
- Logo do cliente: baixado da URL `client.client_logo` e registrado como `"client_logo"` no `ImageRegistry`

#### Seções (`build_question` + lógica no `parse_exam.py`)

- `hide_knowledge_areas_name == True` → NÃO emitir `section.title` (mesmo que a área exista)
- `print_subjects_name == False` → NÃO emitir `section.title` pela disciplina como fallback
- Separador de textos-base (`base_text_location`):
  - `1` → `"beforeQuestion"` (padrão)
  - `2` → `"afterQuestion"`
  - etc. (ver `BASE_TEXT_POSITION_MAP` em `exam_formatter.py`)

#### Parágrafos em enunciados

No banco, o texto vem como HTML. O formatter converte tags `<p>` em separadores de parágrafo. O prova-pdf usa `\n\n` para separar parágrafos (produz `2 × line_height ≈ 33.75pt`). O CSS do Chromium usa `margin-bottom` nos `<p>` que, com margin collapse, produz `~42.82pt` — diferença conhecida de ~9pt por parágrafo que ainda não foi calibrada.

---

## 4. Geração do PDF Candidato

### Script: `run_case.sh` — etapa Go wrapper

O Go wrapper (`packages/go/cmd/generate`) incorpora o WASM via `//go:embed prova_pdf.wasm`. É compilado automaticamente no primeiro `run_case.sh`. **Importante:** se o WASM for recompilado, o binário antigo deve ser apagado:

```bash
rm tests/compare/out/portugues_poema/generate_bin
```

**Compilar WASM após mudanças no Rust:**
```bash
cargo build --target wasm32-wasip1 --features wasi-lib,math,images --no-default-features --release
cp target/wasm32-wasip1/release/prova_pdf.wasm packages/go/provapdf/prova_pdf.wasm
```

---

## 5. Extração de Snapshot (pdf_snapshot.py)

Usa `pdfplumber` para extrair de cada página:

- `text_runs`: grupos de caracteres com mesmo (y, fonte, tamanho, cor) formando uma "linha"
- `rects`: retângulos preenchidos (fundos de alternativas, badges, etc.)
- `lines`: linhas horizontais/verticais
- `images`: imagens embutidas

**Coordenadas:** `y` medido a partir do topo da página (pdfplumber usa `page.height - y1`).

**Nota sobre ligaduras:** pdfplumber fragmenta palavras com ligaduras (fi, fl) — ex: "modificam" vira `"m"` + `"odiﬁcam"`. Isso causa ~900 added/removed no diff mesmo quando o layout está correto. É um artefato do extrator, não do motor.

---

## 6. Comparação (snapshot_diff.py)

Compara `ref.snapshot.json` vs `cand.snapshot.json` elemento por elemento:

- **Matching por chave**: `text_run` é casado por texto normalizado; `rect`/`line`/`image` por posição arredondada
- **Tolerância**: `--tolerance-pt 0.5` — diferenças de posição ≤ 0.5pt são ignoradas
- **Saída**: `diff.json` (estruturado) + `diff.md` (legível)
- **Categorias**:
  - `divergences`: elemento encontrado em ambos, mas com campos diferentes (posição, cor, tamanho)
  - `added`: elemento no candidato que não existe no ref
  - `removed`: elemento no ref que não existe no candidato

**Código de saída:** 0 = zero divergências; 1 = divergências encontradas.

---

## 7. Calibrações Aplicadas

### DPI Scale: Chromium 75% zoom

O Chromium renderiza o PDF de impressão com zoom efetivo de 75% (`72/96 = 0.75`). Toda medida CSS em pixels sofre dupla escala:

```
tamanho_pt = tamanho_px × (72/96) × (72/96) = tamanho_px × 0.5625
```

Isso se aplica a fontes, margens e espaçamentos que derivam de `rem` ou `px`.

**Exemplo:** fonte do header = `0.875rem × 16 = 14px CSS → 14 × 0.5625 = 7.875pt`

**Constante no código Rust:** `src/layout/header.rs` — `BODY_FONT_SIZE_PT = 7.875`

### Labels uppercase

As labels dos campos do header (ALUNO:, Nº:, SÉRIE:, etc.) são renderizadas em uppercase no Chromium (via CSS `text-transform: uppercase`). No prova-pdf isso é replicado com `.to_uppercase()` nos campos de label.

---

## 8. Diferenças Conhecidas e Não Resolvidas

| Diferença | Causa | Magnitude |
|-----------|-------|-----------|
| Espaçamento entre parágrafos | CSS `<p>` margin-bottom com collapse ≈ 9pt extra por parágrafo | ~9pt/§ |
| Fragmentação por ligaduras | pdfplumber divide palavras com fi/fl | ~900 added/removed no diff |
| Cor do texto body | REF usa `#001737` (navy), prova-pdf usa `#000000` | diferença de cor |
| Logo do cliente | Não embutido sem acesso ao banco | imagem ausente |
| Espaço último parágrafo → alternativas | `STEM_BOTTOM_MARGIN_PT` possivelmente insuficiente | ~8pt |

---

## 9. Presets Disponíveis

| Preset | UUID | Descrição |
|--------|------|-----------|
| `portugues_poema` | `2caa96b0-...` | Português com poema, header full, 2 colunas |
| `pga_2em_2trimestre` | `ae5cd60b-...` | PGA 2° EM 2° tri |
| `exatas` | `ae5cd60b-...` | Ciências exatas |
| `matematica_vml` | `ba6442ec-...` | Matemática VML |
| `p4_lingua_portuguesa` | `489dafa5-...` | P4 Língua Portuguesa |

---

## 10. Fluxo Completo de Uma Sessão de Debug

```bash
# 1. Garantir que lize está rodando (para capturar referência)
cd ~/Documentos/Dickson/lize/lizeedu && docker-compose up -d

# 2. Capturar referência se ainda não existe
cd ~/Documentos/Dickson/estudos/prova-pdf/tests
python compare/capture_reference.py portugues_poema

# 3. Gerar spec do banco
python parse_exam.py portugues_poema

# 4. (Se mudou o Rust) Recompilar WASM
cd ~/Documentos/Dickson/estudos/prova-pdf
cargo build --target wasm32-wasip1 --features wasi-lib,math,images --no-default-features --release
cp target/wasm32-wasip1/release/prova_pdf.wasm packages/go/provapdf/prova_pdf.wasm

# 5. Apagar binário cacheado para forçar recompilação
rm -f tests/compare/out/portugues_poema/generate_bin

# 6. Rodar comparação
tests/compare/run_case.sh portugues_poema

# 7. Ver resultado
cat tests/compare/out/portugues_poema/diff.md | head -50
```
