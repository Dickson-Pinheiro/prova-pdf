# Análise da Folha de Respostas (Gabarito) — Referência Chromium/lize

Fonte: `Folha_de_Respostas_Avulsa_P5_MATEMTICA_F7_ANGLO_2026.pdf`, página 1, isolada em
`reference/folha_respostas.pdf`. Metadados extraídos com pdfplumber em `reference/snapshot.json`
(gerado por `snapshot.py`). Todas as medidas em **pt**, origem no **topo-esquerdo** da página.

Página: **594.96 × 841.92 pt** (A4 Chromium). Conteúdo: x = 23 → 573 (largura 550).
Escala CSS→pt observada: **1px = 0.52pt** (bordas 2px = 1.04; fonte 14px = 7.28; fiducial 30px = 15.6; logo 40px alto = 20.79).

## Paleta

| Uso | Cor |
|---|---|
| Texto (todo) | `#001737` (navy lize) |
| Bordas de tabela/caixa | `#999999`, espessura 1.04 (desenhadas como retângulos preenchidos, não stroke) |
| Círculos (bolhas) | stroke `#464646`, ⌀ 9.36 |
| Sombreamento alternado (células) | `#EAEDF3` |
| Caixa de instruções de preenchimento | fill `#DEDEDE`, filete superior 0.52 `#485E90` |
| Separadores da linha-índice invisível da matrícula | `#ADB0B8` (0.52 × 10.92) |
| Módulos do QR | `#000000` |

## Tipografia (IBM Plex Sans)

| Elemento | Tamanho | Peso | Observações |
|---|---|---|---|
| Código de rastreio `#A:1:<uuid>#` | 8.32 (16px) | regular | centrado, top do glifo y=15.83 |
| Instituição, títulos ("Orientações", "Matrícula", "Respostas"), rodapé | 7.28 (14px) | bold (títulos/instituição), regular (rodapé) | |
| Labels do header (UNIDADE:, TURMA:, PROVA:, ALUNO:) | 7.28 | regular; valor de PROVA em **bold** | |
| Corpo (orientações, assinatura, instruções) | 7.62 | regular | line-height 11.44 |
| Dígitos da matrícula, letras A–E, número da questão | 5.54 | número da questão em bold | |

## Estrutura vertical (y do topo)

1. **Código de rastreio** — texto centrado, top 15.83.
2. **Tabela do cabeçalho** — x 23→573, y 26.48→108.61. Bordas 1.04 `#999` como retângulos
   preenchidos. Linhas divisórias em y = 47.27, 67.54, 87.82.
   - Célula do logo: x 23→131.13, atravessa as 4 linhas. Logo centrado: img (28.2, 57.15) 98.25×20.79.
   - Célula do QR: x 496.06→573, atravessa as 4 linhas. QR 29×29 módulos de 1.77
     (51.35 total), centro em (534.53, 67.55), sem quiet zone.
   - Linha 1: instituição bold centrada na célula do meio (x 131.65→496.06), top 33.28.
   - Linha 2: dividida em x≈329.7. "UNIDADE:" x 136.83, "TURMA:" x 335.29, tops 53.55.
   - Linha 3: "PROVA: " regular + título bold, x 136.83, top 73.82.
   - Linha 4: "ALUNO:", x 136.83, top 94.10.
   - Padding esquerdo das células de texto ≈ 5.2.
3. **Fiduciais (alvos ◎)** — 4 imagens de 15.6×15.6 nos cantos da área OMR:
   (25.6, 119.53), (554.81, 119.53), (26.12, 792.73), (554.29, 792.73).
   Forma: alvo concêntrico — anel externo espesso, anel médio, ponto central preenchido.
4. **Painel Orientações** — x 23→422.24, y 116.93→295.76 (bordas brancas = invisíveis).
   - Título "Orientações" bold centrado, top 127.37.
   - Bullets: pontos ⌀2.08 navy em x 42.23 (tops 150.20/184.51/218.82); texto x 49.51,
     largura até ~416.6, tamanho 7.62, fluxo contínuo com pitch 11.44 sem espaço extra
     entre itens (tops das linhas: 146.86, 158.29, 169.73 | 181.17, 192.60, 204.04 | 215.48, 226.91).
   - Linha de assinatura: retângulo 271.36×0.52 navy em (86.94, 280.16); label
     "Assinatura do aluno" 7.62 centrado, top 282.54.
5. **Painel Matrícula** — x 421.72→573.
   - Label "Matrícula" bold centrado no painel, top 123.21.
   - **Linha-índice invisível**: dígitos 0–9 em branco (7.28), top 143.48, um por coluna,
     com separadores verticais 0.52×10.92 `#ADB0B8` em x 450.32 + 11.955k (y 141.88).
   - Grade 10 colunas × 10 linhas: bolha ⌀9.36 stroke `#464646`; x0 = 439.66 + 11.783k
     (medido: pitch alterna 11.44/11.96 por arredondamento de subpixel do Chromium);
     y0 = 155.14 + 13.751k (alterna 13.51/14.05). Dígito da linha (5.54 navy) centrado
     na bolha, top da linha n = y0(n) + 2.17.
   - Sombreamento `#EAEDF3` nas colunas pares (0,2,4,6,8), por célula (~11.96 × 13.52/14.04).
6. **Caixa de instruções de preenchimento** — x 23→573, y 299.92→340.98, fill `#DEDEDE`,
   filete superior 0.52 `#485E90`. Texto 7.62 em x 31.32, tops 311.13/322.56.
   Exemplo (imagem na referência, x 397.29→474.23, y 308.23→333.71): linha "Correto" =
   bolha preenchida + B C D E vazias; linha "Errado" = X, borrão, check, traço sobre a
   letra, ponto pequeno. **No prova-pdf o exemplo é desenhado vetorialmente** (divergência
   proposital: a referência embute um PNG).
7. **Caixa Respostas** — x 23→573, y 349.30→811.45, borda 1.04 `#999`.
   - Título "Respostas" bold centrado, top 356.10.
   - Linhas de resposta a partir de y 373.74, pitch 13.77:
     célula do número x 47.43 largura 19.23 (número bold 5.54 centrado);
     5 células de bolha de 11.96 a partir de x 66.66; bolha ⌀9.36 em x0 = 68.0 + 11.83k;
     letras A–E 5.54 navy, centradas (top = topo da bolha + 2.15).
   - Sombreamento `#EAEDF3` nas linhas ímpares (1ª, 3ª, 5ª…), por célula, incluindo a célula do número.
   - **5ª alternativa oculta**: o template SEMPRE reserva 5 colunas; quando a prova tem 4
     alternativas, a bolha E e sua letra são pintadas na cor do fundo da linha
     (`#EAEDF3` em linha sombreada, `#FFFFFF` caso contrário) — invisíveis, mas presentes.
   - **Múltiplas colunas de questões** (calibrado com a referência ENEM `OUTUBRO`,
     p.1 isolada em `reference/folha_respostas_enem_multicol.pdf`, 90 questões):
     as colunas de questões avançam por um **stride fixo de 98.714 pt** (A-bolhas em
     x = 70.907 + k·98.714), com 30 linhas por coluna. Até **5 colunas** cabem na caixa
     antes de transbordar para uma página de continuação. O passo interno de cada coluna
     (célula do número, 5 bolhas, pitch 11.825, row pitch 13.776) é idêntico ao da coluna 0.
     Verificado: candidato vs. referência com IBM Plex Sans → colunas dentro de 0.05pt e
     stride exato.
8. **Rodapé** — "Lize - 2026" 7.28 regular centrado, top 813.05.

## Modelo de métricas de texto do Chromium (descoberto na calibração)

Derivado por engenharia reversa comparando advances caractere a caractere
(`compare.py` chegou a **0 divergências com tolerância 0.5pt**):

1. **Grid de pixels CSS**: 1px = 0.52pt nesta folha. Tamanhos, bordas e
   posições são múltiplos de px.
2. **Advances hintados por glifo**: o layout usa métricas FreeType a ppem
   inteiro — `ppem = round(size_pt / 0.52)` (corpo 7.62pt = 14.65px → 15ppem)
   e cada advance = `round(hmtx × ppem / upem)` px **inteiros**.
3. **Kerning arredondado separadamente**: ajustes de kern são arredondados a
   px por par — kerns típicos (< 0.5px) viram zero.
4. **Exceções de grid-fitting**: as instruções TrueType desviam do
   arredondamento linear em alguns glifos. Medidos: regular 'V'@14ppem →
   8px (linear 8.526) e bold 'E'@14ppem → 9px (linear 8.498). Tabela
   `HINT_OVERRIDES` em `src/layout/answer_sheet/mod.rs`.
5. **Justificação**: `text-align: justify` distribui o excedente igualmente
   entre os espaços da linha (fracionário), exceto na última linha.
6. **Código de rastreio**: letter-spacing de 0.8px somado a cada advance
   (per-char rounding, sem snap cumulativo).
7. **Fonte**: IBM Plex Sans v3.1 (idêntica à `fonts/` do repo — verificado
   por hmtx).

## Divergências propositais do prova-pdf vs. referência

| Item | Referência | prova-pdf |
|---|---|---|
| Fiduciais | imagem PNG embutida (X4) | desenho vetorial (anéis concêntricos) |
| Exemplo Correto/Errado | imagem PNG embutida (X11) | desenho vetorial |
| **Grade de matrícula** | presente | **não renderizada por padrão** (decisão de 2026-07-09; disponível via `registration` na spec) |
| Página | 594.96×841.92 (Chromium) | A4 595.28×841.89 (dif. < 0.4pt) |
| Bordas | retângulos por aresta | idem (FilledRect por aresta, segmentação exata replicada) |
| ToUnicode de ligaduras | ﬁ → "fi" | ﬁ → U+FB01 (normalizado no compare.py) |

## Incógnitas (a calibrar com referências futuras)

- ~~Quebra em múltiplas colunas da caixa Respostas~~ — **calibrado** (2026-07-10) com a
  referência ENEM de 90 questões: stride fixo de **98.714 pt**, 30 linhas/coluna, até 5
  colunas por página (ver §7). O `COLUMN_STRIDE = largura/4` anterior (137.5) era um chute.
- Reflow do layout quando a matrícula está ausente (mantidas as posições fixas do
  template; o painel direito fica vazio).

## Fluxo de verificação

```bash
cargo test --test answer_sheet_render   # gera out/candidate.pdf
cd tests/answer_sheet && python3 compare.py   # diff vs referência (exit 0 = OK)
```
