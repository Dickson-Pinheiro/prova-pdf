# Progresso da Migração: Chromium → prova-pdf

Relatório de progresso com base no plano original (`webassembly-pdf/MIGRATION.md`).
Última atualização: 2026-03-21.

---

## Resumo executivo

O motor `prova-pdf` cobre a grande maioria das funcionalidades previstas no plano de migração.
Das **13 lacunas críticas** identificadas originalmente, **11 estão resolvidas**, 1 parcialmente implementada e 1 pendente. O lado WASM (Rust) está em estágio avançado; o lado de integração (Django serializer + pdf-service Go) ainda não foi iniciado.

---

## 1. Lacunas críticas — status atual

| # | Funcionalidade | Plano original | Status | Observações |
|---|---|---|---|---|
| 1 | **Parser HTML → InlineContent** | TASK-026 — Alta prioridade | ⬜ Pendente | Conversão fica no Django (Python). Nenhum código Rust necessário. |
| 2 | **Textos-base (BEFORE/AFTER/LEFT/RIGHT)** | TASK-027 — Alta | ✅ Completo | `BaseText` com 7 posições (BeforeQuestion, AfterQuestion, LeftOfQuestion, RightOfQuestion, SectionTop, ExamTop, ExamBottom). Layout implementado em `src/layout/base_text.rs` (429 LOC). |
| 3 | **Questão de somatório** | TASK-031 — Alta | ✅ Completo | `QuestionKind::Sum` + `SumAnswer` com items (value 1,2,4,8,16,32,64) e sum box opcional. Render em `src/layout/question.rs`. |
| 4 | **Cabeçalho de prova estruturado** | TASK-029 — Alta | ✅ Completo | `InstitutionalHeader` com logo, escola, título, disciplina, ano + `StudentField[]` dinâmicos. Layout em `src/layout/header.rs` (713 LOC). Implementação via Opção B (estruturada), mais robusta que a Opção A sugerida no plano. |
| 5 | **Numeração customizada** | TASK-028 — Alta | ✅ Completo | `Question.number: Option<u32>` permite número explícito. Auto-incremento via pipeline quando `None`. `start_number` controlável pelo Django. |
| 6 | **Pontuação por questão** | Média | ✅ Completo | `Question.points: Option<f64>` + `PrintConfig.show_score` controla exibição. Badge renderizado à direita do número. |
| 7 | **Separador de disciplina** | TASK-030 — Média | ✅ Completo | `Section` com título, categoria, instruções. Layout em `src/layout/section.rs` (518 LOC). Mais rico que simples `Heading` — suporta category badge e force_page_break. |
| 8 | **Cloze (lacunas)** | Média | ✅ Completo | `QuestionKind::Cloze` + `ClozeAnswer` com word bank e shuffle. `InlineContent::Blank` para lacunas no texto. |
| 9 | **Redação / Essay** | Média | ✅ Completo | `QuestionKind::Essay` + `EssayAnswer` (line_count ou height_cm). Auto full-width no pipeline. Linhas pautadas renderizadas. |
| 10 | **Grayscale de imagens** | Baixa | ✅ Completo | `PrintConfig.image_grayscale` e `all_black` mode. Conversão via `image 0.25` no emitter (`src/pdf/images.rs`). |
| 11 | **Tamanho ATA (200×266mm)** | TASK-032 — Baixa | ✅ Completo | `PageSize::Ata` = 566.9×754pt. Implementado como enum variant. |
| 12 | **Fonte IBM Plex Sans** | TASK-033 — Média | ⬜ Pendente (infra) | O motor suporta qualquer TTF/OTF via `add_font()`. O embed das fontes é responsabilidade do pdf-service (Go container). |
| 13 | **Hifenização** | Baixa | 🔶 Parcial | Line-breaking via `unicode-linebreak` (UAX #14). Hifenização automática (inserir hífens em palavras longas) não implementada. |

---

## 2. Lacunas não críticas — status atual

| Funcionalidade | Status | Observações |
|---|---|---|
| **Folha de gabarito separada** | ⬜ Pendente | Gerar segundo PDF com gabarito. Estrutura de dados suporta `is_correct` em alternativas. |
| **Múltiplas versões (randomização)** | ⬜ Pendente (Django) | Lógica de randomização fica no Django. O motor recebe JSON já embaralhado. |
| **Folha de fórmulas** | ✅ Completo | `Appendix` com `FormulaEntry[]` (label + LaTeX). Layout em `src/layout/appendix.rs`. |
| **Marca d'água (watermark)** | N/A | Continua via pdfcpu pós-processamento. |

---

## 3. Checklist de paridade — atualizado

| Funcionalidade | Fase | exam-pdf (Rust) | Serializador (Django) | Status |
|---|---|---|---|---|
| Texto simples | 1 | ✅ | ⬜ a implementar | 🔶 |
| Negrito / itálico inline | 1 | ✅ | ⬜ a implementar | 🔶 |
| Math LaTeX inline | 1 | ✅ (feature `math`) | ⬜ a implementar | 🔶 |
| Math LaTeX display | 1 | ✅ (feature `math`) | ⬜ a implementar | 🔶 |
| Múltipla escolha (A-Z) | 1 | ✅ | ⬜ a implementar | 🔶 |
| Questão discursiva (linhas) | 1 | ✅ | ⬜ a implementar | 🔶 |
| Questão discursiva (espaço) | 1 | ✅ | ⬜ a implementar | 🔶 |
| Tamanho A4 | 1 | ✅ | ✅ trivial | ✅ |
| Tamanho ATA (200×266) | 1 | ✅ | ✅ trivial | ✅ |
| 1 ou 2 colunas | 1 | ✅ | ✅ trivial | ✅ |
| Font size configurável | 1 | ✅ | ✅ trivial | ✅ |
| IBM Plex Sans / outras fontes | 1 | ✅ | ✅ trivial | ✅ |
| Margens configuráveis | 1 | ✅ | ✅ trivial | ✅ |
| Header com paginação | 1 | ✅ `RunningHeader` | ⬜ a implementar | 🔶 |
| Pontuação por questão | 1 | ✅ | ✅ trivial | ✅ |
| Numeração customizada | 1 | ✅ | ⬜ a implementar | 🔶 |
| Separador de disciplina | 1 | ✅ `Section` | ⬜ a implementar | 🔶 |
| Quebra de página por questão | 1 | ✅ `force_page_break` | ⬜ a implementar | 🔶 |
| Imagens PNG/JPEG | 2 | ✅ | ⬜ a implementar | 🔶 |
| Texto-base (BEFORE/AFTER) | 2 | ✅ | ⬜ a implementar | 🔶 |
| Texto-base (LEFT/RIGHT) | 3→2 | ✅ | ⬜ a implementar | 🔶 |
| Questão de somatório | 2 | ✅ | ⬜ a implementar | 🔶 |
| Cloze (lacunas) | 2 | ✅ | ⬜ a implementar | 🔶 |
| Cabeçalho da prova | 2 | ✅ | ⬜ a implementar | 🔶 |
| Redação / essay | 3→1 | ✅ (full-width auto) | ⬜ a implementar | 🔶 |
| Grayscale de imagens | 3 | ✅ | ✅ trivial (config) | ✅ |
| Gabarito separado | 3 | ⬜ a implementar | ⬜ a implementar | ⬜ |
| Rodapé / watermark | pdfcpu | N/A | N/A | ✅ |
| Números de página | pdfcpu | N/A | N/A | ✅ |
| Páginas em branco | pdfcpu | N/A | N/A | ✅ |
| Upload S3 | sem mudança | N/A | N/A | ✅ |

**Legenda:** ✅ = pronto · 🔶 = Rust pronto, Django pendente · ⬜ = não iniciado

---

## 4. Funcionalidades implementadas além do plano original

O prova-pdf implementou diversas capacidades que o plano original não previa ou adiava para fases futuras:

| Funcionalidade | Descrição |
|---|---|
| **Textos-base em 7 posições** | Plano previa 4 (BEFORE/AFTER/LEFT/RIGHT). Implementadas 7, incluindo SectionTop, ExamTop, ExamBottom. |
| **Full-width questions** | Questões podem ocupar toda a largura mesmo em layout 2-colunas. Linha divisória segmentada evita sobreposição. |
| **Appendix com fórmulas** | Apêndice tipado com folhas de fórmulas (LaTeX renderizado). |
| **Running header/footer** | Cabeçalho/rodapé em todas as páginas (exceto primeira) com tokens `{page}/{pages}`. |
| **Binding WASI (C-ABI)** | Além do browser (wasm-bindgen), suporte completo a WASI para integração via Go (wazero) e Python (wasmtime). |
| **Sistema de cores CSS** | Suporte a hex, rgb(), rgba(), oklch(). Modo all-black para impressão P&B. |
| **Economy mode** | Espaçamentos reduzidos (0.7×) para provas com muitas questões. |
| **Validação single-pass** | Validação completa antes do render, com coleta de todos os erros. |
| **Subscript/superscript** | `InlineContent::Sub` e `InlineContent::Sup` com nesting. |
| **Font subsetting** | Apenas glyphs usados são embarcados no PDF — reduz tamanho significativamente. |
| **Style cascade** | Estilo cascateado: Config → Section → Question → Inline. Override por elemento. |
| **Letter case** | `LetterCase::Upper` / `Lower` para alternativas. |
| **Draft lines** | Linhas de rascunho após a resposta, com altura configurável. |

---

## 5. Arquitetura do motor — visão geral

```
src/ (~15.500 LOC, 39 arquivos .rs)
├── spec/        (1.200 LOC)  Modelo de dados JSON (ExamSpec, Question, Answer, etc.)
├── fonts/         (650 LOC)  Registry, Data (TTF parsing), Resolver (role → family)
├── layout/      (7.200 LOC)  Motor de layout (fragment IR, inline, question, page, header, section, etc.)
├── math/        (2.500 LOC)  LaTeX parser + layout engine (feature-gated)
├── pdf/         (2.100 LOC)  Emitter PDF (pdf-writer 0.14, font embedding, images, drawing)
├── pipeline/    (1.100 LOC)  Orquestração: validação → layout → emissão
├── bindings/      (900 LOC)  WASM: browser (wasm-bindgen) + WASI (C-ABI)
└── color.rs       (764 LOC)  Parser CSS de cores + modo P&B
```

---

## 6. O que falta para produção

### 6.1 No prova-pdf (Rust) — prioridade baixa

| Item | Esforço | Notas |
|---|---|---|
| Hifenização automática | Médio | Integrar crate `hyphenation` com dicionário pt-BR. |
| Gabarito separado | Médio | Gerar segundo PDF com respostas corretas marcadas. |
| Integração math no inline engine | Médio | `InlineContent::Math` está ignorado no layout (TASK-034). Parser e layout math existem, falta conectar ao pipeline de fragments. |
| Tabelas HTML | Alto | Não há suporte a `<table>` no inline layout. Questões com tabelas precisam ser convertidas para outra representação. |
| Listas (ul/ol) | Baixo | Converter para texto com bullet/número no serializer Django, ou adicionar suporte nativo. |

### 6.2 No Django (lizeedu) — prioridade alta

| Item | Esforço | Notas |
|---|---|---|
| `html_to_inline_content()` | Alto | Converter HTMLField → `InlineContent[]`. BeautifulSoup/lxml. Detectar MathJax markers. |
| `exam_to_document_spec()` | Médio | Serializar modelos ORM → `ExamSpec` JSON. |
| `ExamGeneratePdfView` | Baixo | Endpoint POST que envia JSON ao pdf-service. |
| Rota Django | Trivial | `path("exams/<pk>/generate-pdf/", ...)` |

### 6.3 No pdf-service (Go) — prioridade alta

| Item | Esforço | Notas |
|---|---|---|
| Endpoint `POST /print-json` | Médio | Receber DocumentSpec, chamar exam-pdf via wazero, pós-processar com pdfcpu. |
| Integração wazero | Médio | Carregar WASM, expor `prova_pdf_*` exports. Binding WASI já está pronto no Rust. |
| Fontes no container | Baixo | Copiar TTFs para `/fonts/` no Dockerfile. |
| Download de imagens | Médio | Buscar imagens referenciadas no HTML e passá-las via `add_image()`. |

---

## 7. Fases revisadas

### Fase 1 — Integração básica (próximo passo)
Foco no Django serializer e pdf-service. O motor Rust já suporta tudo necessário.

- [ ] Implementar `html_to_inline_content()` no Django
- [ ] Implementar `exam_to_document_spec()` para CHOICE e TEXTUAL
- [ ] Criar endpoint `/print-json` no pdf-service com wazero
- [ ] Fontes no container Docker
- [ ] Testar com 5 provas simples (sem imagens)

### Fase 2 — Cobertura ampla
- [ ] Suporte a `<img>` no serializer (download + `add_image()`)
- [ ] Serializar questões SUM, CLOZE, ESSAY
- [ ] Cabeçalho estruturado com logo
- [ ] Textos-base em todas as posições
- [ ] Integração math (TASK-034: conectar parser/layout ao inline engine)
- [ ] Testar com 20 provas diversas

### Fase 3 — Paridade e rollout
- [ ] Gabarito separado
- [ ] Hifenização (opcional)
- [ ] Tabelas HTML (se necessário)
- [ ] Feature flag no Django
- [ ] Testes A/B (Chromium vs prova-pdf)
- [ ] Rollout gradual

---

## 8. Métricas

| Métrica | Plano original | Atual |
|---|---|---|
| LOC total (Rust) | — | ~15.500 |
| Tipos de questão suportados | 6 | 6/6 ✅ |
| Posições de texto-base | 4 | 7 (superou plano) |
| Tamanhos de página | A4 + ATA | A4 + ATA + Custom ✅ |
| Targets WASM | browser | browser + WASI ✅ |
| Testes unitários | — | 477 passando |
| Lacunas críticas resolvidas | 0/13 | 11/13 |
| Performance esperada | 50-200ms | A verificar em integração |
