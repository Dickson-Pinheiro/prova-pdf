# prova-pdf

Gerador de PDF de provas acadêmicas compilado para **WebAssembly**. Recebe um JSON estruturado (`ExamSpec`), retorna bytes de PDF. Zero dependência de DOM, browser ou Chromium.

**Suporte nativo a:** 6 tipos de questão (objetiva, dissertativa, somatório, cloze, redação, arquivo), textos-base em 7 posições, cabeçalho institucional, layout bicolunado, fórmulas LaTeX e fontes TTF/OTF.

## Instalação

```bash
# Node.js / Browser
npm install prova-pdf

# Python
pip install prova-pdf

# Go
go get github.com/Dickson-Pinheiro/prova-pdf/packages/go/provapdf
```

## Uso — Browser

```html
<script type="module">
  import init, { add_font, generate_pdf } from "https://cdn.jsdelivr.net/npm/prova-pdf@0.1.2/prova_pdf.js";

  await init();

  // Registrar fonte (obrigatório: pelo menos "body" regular)
  const fontRes = await fetch("/fonts/DejaVuSans.ttf");
  add_font("body", 0, new Uint8Array(await fontRes.arrayBuffer()));

  // Gerar PDF
  const pdf = generate_pdf({
    sections: [{
      title: "Matemática",
      questions: [{
        kind: "choice",
        stem: [{ type: "text", value: "Quanto é 2 + 2?" }],
        answer: {
          type: "choice",
          alternatives: [
            { label: "A", content: [{ type: "text", value: "3" }] },
            { label: "B", content: [{ type: "text", value: "4" }] },
            { label: "C", content: [{ type: "text", value: "5" }] },
          ],
        },
      }],
    }],
  });

  // Download
  const blob = new Blob([pdf], { type: "application/pdf" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = "prova.pdf";
  a.click();
</script>
```

## Uso — Node.js (TypeScript)

```typescript
import { readFileSync, writeFileSync } from "fs";
import { createRequire } from "module";
import { initSync, add_font, generate_pdf, clear_all } from "prova-pdf";
import type { ExamSpec, ChoiceAnswer } from "prova-pdf/types";

const require = createRequire(import.meta.url);
const wasmBytes = readFileSync(require.resolve("prova-pdf/prova_pdf_bg.wasm"));
initSync({ module: wasmBytes });

clear_all();
add_font("body", 0, new Uint8Array(readFileSync("DejaVuSans.ttf")));

const spec: ExamSpec = {
  config: { columns: 2, fontSize: 11 },
  header: {
    institution: "Escola Municipal",
    title: "Prova de Ciências",
    studentFields: [{ label: "Nome" }, { label: "Turma", widthCm: 5 }],
  },
  sections: [{
    title: "Questões Objetivas",
    questions: [{
      kind: "choice",
      stem: [{ type: "text", value: "Qual é o maior planeta do sistema solar?" }],
      answer: {
        type: "choice",
        alternatives: [
          { label: "A", content: [{ type: "text", value: "Terra" }] },
          { label: "B", content: [{ type: "text", value: "Júpiter" }] },
          { label: "C", content: [{ type: "text", value: "Saturno" }] },
        ],
      } satisfies ChoiceAnswer,
      points: 1.0,
    }],
  }],
};

writeFileSync("prova.pdf", generate_pdf(spec));
```

## Uso — Python

```python
from pathlib import Path
from prova_pdf import generate_pdf

font = Path("DejaVuSans.ttf").read_bytes()

spec = {
    "config": {"columns": 2, "fontSize": 11},
    "header": {
        "institution": "Escola Municipal",
        "title": "Prova de Ciências",
        "studentFields": [{"label": "Nome"}, {"label": "Turma", "widthCm": 5}],
    },
    "sections": [{
        "title": "Questões Objetivas",
        "questions": [{
            "kind": "choice",
            "stem": [{"type": "text", "value": "Qual é o maior planeta do sistema solar?"}],
            "answer": {
                "type": "choice",
                "alternatives": [
                    {"label": "A", "content": [{"type": "text", "value": "Terra"}]},
                    {"label": "B", "content": [{"type": "text", "value": "Júpiter"}]},
                    {"label": "C", "content": [{"type": "text", "value": "Saturno"}]},
                ],
            },
            "points": 1.0,
        }],
    }],
}

pdf = generate_pdf(spec, fonts=[{"family": "body", "variant": 0, "data": font}])
Path("prova.pdf").write_bytes(pdf)
```

## Uso — Go

```go
package main

import (
    "os"
    "github.com/Dickson-Pinheiro/prova-pdf/packages/go/provapdf"
)

func main() {
    fontBytes, _ := os.ReadFile("DejaVuSans.ttf")

    spec := provapdf.ExamSpec{
        Config: provapdf.PrintConfig{
            Columns:  u8Ptr(2),
            FontSize: f64Ptr(11),
        },
        Header: provapdf.InstitutionalHeader{
            Institution: strPtr("Escola Municipal"),
            Title:       strPtr("Prova de Ciências"),
            StudentFields: []provapdf.StudentField{
                {Label: "Nome"},
                {Label: "Turma", WidthCm: f64Ptr(5)},
            },
        },
        Sections: []provapdf.Section{{
            Title: strPtr("Questões Objetivas"),
            Questions: []provapdf.Question{{
                Kind: provapdf.QuestionKindChoice,
                Stem: []provapdf.InlineContent{
                    provapdf.TextContent("Qual é o maior planeta do sistema solar?"),
                },
                Answer: provapdf.AnswerSpace{
                    Type: "choice",
                    Alternatives: []provapdf.Alternative{
                        {Label: "A", Content: []provapdf.InlineContent{provapdf.TextContent("Terra")}},
                        {Label: "B", Content: []provapdf.InlineContent{provapdf.TextContent("Júpiter")}},
                        {Label: "C", Content: []provapdf.InlineContent{provapdf.TextContent("Saturno")}},
                    },
                },
                Points: f64Ptr(1.0),
            }},
        }},
    }

    pdf, _ := provapdf.GeneratePDF(spec, []provapdf.FontInput{
        {Family: "body", Variant: 0, Data: fontBytes},
    })

    os.WriteFile("prova.pdf", pdf, 0644)
}

func strPtr(s string) *string   { return &s }
func f64Ptr(f float64) *float64 { return &f }
func u8Ptr(n uint8) *uint8      { return &n }
```

## Tipos de Questão

| Tipo | `kind` | Descrição |
|------|--------|-----------|
| Objetiva | `choice` | Alternativas A/B/C/D/E com conteúdo inline |
| Dissertativa | `textual` | Linhas ou espaço em branco para resposta |
| Somatório | `sum` | Itens com valores 01/02/04/08/16 e caixa de soma |
| Cloze | `cloze` | Lacunas inline no enunciado + banco de palavras |
| Redação | `essay` | Espaço grande com muitas linhas |
| Arquivo | `file` | Placeholder de upload digital |

## Conteúdo Inline

O campo `stem` e os conteúdos de alternativas usam `InlineContent[]`:

```json
[
  { "type": "text", "value": "A fórmula é ", "style": { "bold": true } },
  { "type": "math", "latex": "E = mc^2", "display": false },
  { "type": "image", "key": "fig1", "widthCm": 8 },
  { "type": "sub", "content": [{ "type": "text", "value": "2" }] },
  { "type": "sup", "content": [{ "type": "text", "value": "n" }] },
  { "type": "blank", "widthCm": 4.0 }
]
```

## Fontes

Registre pelo menos a família `"body"` (variante 0 = regular) antes de gerar. Variantes: `0` regular, `1` bold, `2` italic, `3` bold-italic.

```javascript
add_font("body", 0, regularBytes);
add_font("body", 1, boldBytes);
add_font("heading", 0, headingBytes);
set_font_rules({ body: "body", heading: "heading" });
```

## PrintConfig

| Campo | Tipo | Default | Descrição |
|-------|------|---------|-----------|
| `pageSize` | `"A4"` \| `"Ata"` \| `{widthMm, heightMm}` | `"A4"` | Tamanho da página |
| `columns` | `1` \| `2` | `1` | Número de colunas |
| `fontSize` | number | `12` | Tamanho base em pt |
| `lineSpacing` | `"normal"` \| `"oneAndHalf"` \| `"twoAndHalf"` \| `"threeAndHalf"` | `"normal"` | Espaçamento |
| `margins` | `{top, bottom, left, right}` | `0.6/0.6/1.5/1.5` | Margens em cm |
| `economyMode` | boolean | `false` | Força 2 colunas, reduz espaços |
| `allBlack` | boolean | `false` | Força todas as cores para preto |
| `showScore` | boolean | `false` | Exibe pontuação por questão |
| `breakAllQuestions` | boolean | `false` | Quebra de página antes de cada questão |
| `imageGrayscale` | boolean | `false` | Converte imagens para escala de cinza |

Referência completa em [PROJECT.md](PROJECT.md#8-printconfig--referência-completa).

## Performance

| Cenário | Meta | vs Chromium |
|---------|------|-------------|
| 50 questões sem math | < 50ms | 150-300x mais rápido |
| 50 questões com LaTeX | < 200ms | 40-75x mais rápido |
| Prova completa com imagens | < 400ms | 20-40x mais rápido |

WASM gzipped: ~793 KB (browser) / ~770 KB (WASI).

## Arquitetura

```
ExamSpec (JSON) → Validação → Cascata de Estilo → Layout → Emissão PDF → Vec<u8>
```

4 fases, ~15.500 LOC Rust, 486 testes. Detalhes em [ARCHITECTURE.md](ARCHITECTURE.md).

## Desenvolvimento

```bash
# Testes unitários
cargo test

# Build browser (pkg/)
make build-browser

# Build WASI (wasm/)
make build-wasi

# Teste cross-platform (Python + Node.js + Go)
bash tests/cross-platform/run.sh

# Benchmarks
cargo bench
```

## Licença

MIT
