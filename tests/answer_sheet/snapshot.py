#!/usr/bin/env python3
"""Extrai snapshot estrutural de um PDF de folha de respostas (gabarito).

Mesma abordagem do antigo tests/compare/pdf_snapshot.py (ver COMPATIBILITY_STRATEGY.md):
usa pdfplumber para extrair text_runs, rects, lines, curves e images de cada página,
com coordenadas medidas a partir do TOPO da página, em pt.

Uso:
    python3 snapshot.py reference/folha_respostas.pdf > reference/snapshot.json
"""

import json
import sys

import pdfplumber


def group_text_runs(page):
    """Agrupa chars com mesmo (linha y, fonte, tamanho, cor) em runs."""
    runs = []
    current = None
    for ch in sorted(page.chars, key=lambda c: (round(c["top"], 1), c["x0"])):
        key = (round(ch["top"], 1), ch.get("fontname"), round(ch.get("size", 0), 2),
               str(ch.get("non_stroking_color")))
        if current and current["_key"] == key and ch["x0"] - current["x1"] < ch.get("size", 10):
            current["text"] += ch["text"]
            current["x1"] = round(ch["x1"], 2)
        else:
            if current:
                runs.append(current)
            current = {
                "_key": key,
                "text": ch["text"],
                "x0": round(ch["x0"], 2),
                "x1": round(ch["x1"], 2),
                "top": round(ch["top"], 2),
                "size": round(ch.get("size", 0), 2),
                "font": ch.get("fontname"),
                "color": str(ch.get("non_stroking_color")),
            }
    if current:
        runs.append(current)
    for r in runs:
        del r["_key"]
    return runs


def snapshot_page(page):
    return {
        "width": round(float(page.width), 2),
        "height": round(float(page.height), 2),
        "text_runs": group_text_runs(page),
        "rects": [
            {
                "x0": round(r["x0"], 2), "top": round(r["top"], 2),
                "x1": round(r["x1"], 2), "bottom": round(r["bottom"], 2),
                "w": round(r["x1"] - r["x0"], 2), "h": round(r["bottom"] - r["top"], 2),
                "stroke": r.get("stroke"), "fill": r.get("fill"),
                "stroke_color": str(r.get("stroking_color")),
                "fill_color": str(r.get("non_stroking_color")),
                "linewidth": r.get("linewidth"),
            }
            for r in page.rects
        ],
        "lines": [
            {
                "x0": round(l["x0"], 2), "top": round(l["top"], 2),
                "x1": round(l["x1"], 2), "bottom": round(l["bottom"], 2),
                "stroke_color": str(l.get("stroking_color")),
                "linewidth": l.get("linewidth"),
            }
            for l in page.lines
        ],
        "curves": [
            {
                "x0": round(c["x0"], 2), "top": round(c["top"], 2),
                "x1": round(c["x1"], 2), "bottom": round(c["bottom"], 2),
                "w": round(c["x1"] - c["x0"], 2), "h": round(c["bottom"] - c["top"], 2),
                "stroke": c.get("stroke"), "fill": c.get("fill"),
                "stroke_color": str(c.get("stroking_color")),
                "fill_color": str(c.get("non_stroking_color")),
                "linewidth": c.get("linewidth"),
            }
            for c in page.curves
        ],
        "images": [
            {
                "x0": round(i["x0"], 2), "top": round(i["top"], 2),
                "x1": round(i["x1"], 2), "bottom": round(i["bottom"], 2),
                "w": round(i["x1"] - i["x0"], 2), "h": round(i["bottom"] - i["top"], 2),
                "name": i.get("name"),
            }
            for i in page.images
        ],
    }


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    with pdfplumber.open(sys.argv[1]) as pdf:
        out = {"pages": [snapshot_page(p) for p in pdf.pages]}
    json.dump(out, sys.stdout, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
