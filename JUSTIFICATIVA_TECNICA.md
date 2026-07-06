# Justificativa Técnica: Rust + WebAssembly no prova-pdf

## 1. Contexto e problema

O fluxo atual de geração de PDFs na plataforma lizeedu utiliza **Chromium headless** para renderizar a prova como HTML e então exportar para PDF. Esse processo apresenta os seguintes problemas mensurados:

| Métrica | Chromium headless | Meta prova-pdf |
|---|---|---|
| Tempo por prova (50 questões) | 8–15 segundos | < 200ms |
| Dependência de infraestrutura | Chromium instalado no servidor | Zero — WASM portátil |
| Custo de memória por geração | ~150–300MB (processo Chromium) | < 30MB (WASM runtime) |
| Consistência entre ambientes | Depende da versão do Chromium | Byte-a-byte idêntico em todos os runtimes |

A substituição exige uma solução que rode diretamente no browser do usuário e nos servidores Python/Go sem instalar dependências nativas — cenário ideal para WebAssembly.

---

## 2. Por que Rust

### 2.1 Desempenho sem garbage collector

Rust compila para código nativo sem GC (garbage collector). Em um gerador de PDF, isso é determinante: a geração envolve alocação intensiva de buffers, shaping de glifos por character, e compressão de streams. Linguagens com GC (Go, Java, JavaScript) introduzem pausas imprevisíveis que impossibilitam cumprir a meta de < 200ms de forma confiável.

Benchmark interno comparando o antecessor `exam-pdf` (também Rust/WASM) contra o Chromium headless:

```
exam-pdf (WASM)  → 120ms média para prova de 50 questões com math
Chromium headless → 9.400ms média para a mesma prova
Speedup medido: ~78×
```

### 2.2 Tamanho do binário — `opt-level = "z"` e LTO

Rust oferece controle preciso sobre o binário gerado:

| Configuração aplicada | Efeito no tamanho |
|---|---|
| `opt-level = "z"` | Otimiza para tamanho mínimo (em vez de velocidade) |
| `lto = true` | Link-Time Optimization elimina código morto entre crates |
| `panic = "abort"` | Remove tabelas de unwind (~15–30KB a menos) |
| `strip = true` | Remove símbolos de debug do binário final |
| `wasm-opt -Oz` (binaryen) | Pós-processamento: reduz ainda mais via otimizações WASM-específicas |
| `subsetter` (subsetting de fontes) | Embarca no PDF apenas os glifos efetivamente usados |

Resultado: o binário WASM gzipped fica abaixo de 900KB com todas as features (math + images), enquanto uma solução equivalente em Go ou JavaScript carregaria dezenas de megabytes por transitividade de dependências.

### 2.3 Segurança de memória sem custo de runtime

O modelo de ownership do Rust elimina classes inteiras de bugs (use-after-free, double-free, data races) em tempo de compilação, sem overhead de runtime. Para um módulo WASM que processa dados arbitrários de usuário (fontes TTF, imagens, JSON), isso é especialmente relevante: não há risco de corrupção de memória que poderia comprometer o processo host (browser ou servidor).

### 2.4 Zero-cost abstractions e inlining agressivo

O compilador Rust faz inlining agressivo de traits e genéricos, eliminando vtables em hot paths. No prova-pdf, o dispatch de `QuestionKind` e `FragmentKind` é feito via `enum + match` exaustivo — o compilador garante cobertura de todos os casos e gera código sem indireção de ponteiro, ao contrário de trait objects (`dyn Trait`).

### 2.5 Ecossistema WASM maduro

Rust possui o ecossistema WASM mais maduro dentre linguagens de sistemas:

- **`wasm-bindgen`** — interoperabilidade direta com JavaScript sem camada de glue manual
- **`wasm32-unknown-unknown`** — target sem libc para o menor binário possível (browser)
- **`wasm32-wasip1`** — target WASI para runtimes como wasmtime (Python) e wazero (Go)
- Suporte oficial do time Rust (Cargo, rustup, rustc) sem toolchains experimentais

---

## 3. Por que WebAssembly

### 3.1 Um único binário, três runtimes

O objetivo do prova-pdf é rodar identicamente em:

| Runtime | Target WASM | Uso |
|---|---|---|
| Browser (Chrome, Firefox, Safari) | `wasm32-unknown-unknown` + wasm-bindgen | Geração client-side sem round-trip ao servidor |
| Python (lizeedu backend) | `wasm32-wasip1` via wasmtime | Geração server-side no Django |
| Go (serviços internos) | `wasm32-wasip1` via wazero | Geração server-side em microsserviços Go |

Sem WASM, seria necessário manter três implementações separadas (JS, Python nativo, Go nativo) ou um serviço HTTP intermediário com latência adicional e ponto único de falha.

### 3.2 Isolamento de segurança nativo

O runtime WASM executa o módulo em uma sandbox com memória linear isolada. O módulo não tem acesso ao sistema de arquivos, rede, ou qualquer API do host que não seja explicitamente exposta. Isso é especialmente importante ao processar arquivos de fonte e imagens enviados por usuários.

### 3.3 Tamanho do build e carregamento

O binário WASM é compacto e transferido uma única vez, sendo cacheado pelo browser:

| Configuração de features | Tamanho gzipped (meta) |
|---|---|
| Padrão (math + images) | < 900KB |
| Sem images | < 750KB |
| Mínimo (sem math, sem images) | < 500KB |

Para referência, a biblioteca Puppeteer (wrapper do Chromium headless) tem ~300MB instalada. Um bundle React típico com jsPDF fica em 2–4MB não comprimido, com limitações severas de tipografia.

### 3.4 Determinismo e reprodutibilidade

WASM é determinístico por especificação: dado o mesmo input, o mesmo módulo produz o mesmo output em qualquer plataforma (x86, ARM, browser, servidor). Isso garante que a prova gerada no browser seja byte-a-byte idêntica à gerada no servidor, eliminando discrepâncias de renderização que existiam com o Chromium headless (cujo output variava entre versões).

### 3.5 Sem dependências de sistema operacional

O módulo WASM compilado contém todo o código necessário para geração de PDF, incluindo:

- Motor de layout de texto (inline engine com quebra de linha UAX #14)
- Shaping OpenType (HarfBuzz-compatível via rustybuzz)
- Subsetting de fontes
- Compressão Deflate (miniz_oxide)
- Parser LaTeX para math (pulldown-latex)
- Decodificação de imagens PNG/JPEG

Não há dependências de libfreetype, libharfbuzz, ghostscript, ou qualquer biblioteca do sistema. O deploy resume-se a copiar um arquivo `.wasm` e um arquivo `.js` de glue.

---

## 4. Comparativo com alternativas

---

## 6. Benchmarks medidos (execução nativa, `cargo bench`)

Resultados coletados em 2026-03-23 com Criterion.rs (100 amostras cada), hardware: Linux x86_64.

### 6.1 Pipeline end-to-end — geração completa de PDF

| Cenário | Tempo médio | Meta do projeto | Resultado |
|---|---|---|---|
| 10 questões choice, 2 colunas | **22,5ms** | < 50ms | ✓ dentro da meta |
| 50 questões choice, 2 colunas | **111ms** | < 200ms | ✓ dentro da meta |
| 100 questões choice, 2 colunas | **222ms** | — | referência |
| Fixture `all_kinds` (1 de cada tipo) | **4,5ms** | — | referência |

Contexto: o fluxo atual com Chromium headless leva **8.000–15.000ms** para 50 questões — o prova-pdf é **72–135× mais rápido** no mesmo cenário.

### 6.2 Micro-benchmarks — hot paths do motor de layout

| Operação | Tempo médio | Observação |
|---|---|---|
| `text_width` — texto curto (10 chars) | **1,65µs** | Chamado por token no layout inline |
| `text_width` — texto longo (130 chars) | **9,67µs** | |
| `glyph_id` — lookup de glifo | **1,01µs** | Chamado por caractere |
| `shape_text` — shaping curto | **31,98µs** | HarfBuzz-compatível via rustybuzz |
| `shape_text` — shaping longo (150 chars) | **108,9µs** | |
| `shaped_text_width` — largura pós-shaping | **50ns** | Loop sobre advances, sem alocação |

O shaping de texto (operação mais cara por linha) leva menos de **110µs** para uma linha longa — permitindo centenas de linhas por segundo com folga.

### 6.3 Tamanho do binário WASM (medido)

| Artefato | Tamanho bruto | Tamanho gzipped | Meta |
|---|---|---|---|
| Browser (`pkg/prova_pdf_bg.wasm`) | 1,9MB | **760KB** | < 900KB ✓ |
| WASI Python/Go (`wasm/prova_pdf.wasm`) | 2,0MB | **794KB** | < 900KB ✓ |

---

## 7. Comparativo com alternativas

| Alternativa | Problema |
|---|---|
| **jsPDF / pdfmake (JavaScript puro)** | Sem suporte a OpenType shaping; math precisa de KaTeX rodando no browser; fontes embutidas aumentam o bundle para vários MB; sem WASI para servidores Python/Go |
| **Chromium headless (atual)** | 8–15s por geração; ~300MB de dependência; resultado não determinístico entre versões; impossível rodar client-side |
| **LaTeX (pdflatex/xelatex)** | Latência ainda maior; requer instalação de ~1GB de texlive; impossível rodar no browser |
| **Go + UniPDF / gofpdf** | Suporte a math limitado; sem runtime browser; tamanho do binário maior que WASM Rust para funcionalidade equivalente |
| **Python (reportlab / weasyprint)** | WeasyPrint depende de Cairo/Pango/GTK; reportlab sem suporte a math; nenhum dos dois roda no browser |
| **Typst (Rust/WASM)** | Solução genérica de tipografia; não modelaria o domínio de provas sem camada extra; schema próprio incompatível com ExamSpec |

---

## 8. Resumo quantitativo

| Critério | Solução atual (Chromium) | prova-pdf (Rust/WASM) | Melhoria |
|---|---|---|---|
| Tempo de geração (50 questões) | 8.000–15.000ms | < 200ms | **40–75×** |
| Dependência de instalação | ~300MB Chromium | 0 (WASM portátil) | Eliminada |
| Tamanho do artefato transferido | N/A (server-side only) | < 900KB gzipped | Client-side viável |
| Plataformas suportadas | Servidor Linux | Browser + Python + Go | +2 runtimes |
| Determinismo de output | Não (varia por versão) | Sim (byte-a-byte) | Garantido |
| Acesso ao sistema de arquivos | Sim (risco) | Não (sandbox WASM) | Isolamento total |
