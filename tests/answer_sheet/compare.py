#!/usr/bin/env python3
"""Compara o gabarito gerado pelo prova-pdf com a referência Chromium.

Mesma filosofia do antigo snapshot_diff.py (COMPATIBILITY_STRATEGY.md §6):
extrai snapshots de ambos os PDFs (via snapshot.py) e casa elementos com
tolerância posicional.

Uso:
    cargo test --test answer_sheet_render          # gera out/candidate.pdf
    python3 compare.py [--tolerance-pt 0.5]

Saída: resumo no stdout + out/diff.md. Exit 0 = sem divergências fora das
divergências propositais documentadas em ANALYSIS.md; 1 caso contrário.
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).parent

# Divergências propositais (ANALYSIS.md): a referência embute imagens para
# fiduciais e para o exemplo Correto/Errado; o prova-pdf desenha vetores.
# A grade de matrícula existe na referência mas foi removida do produto
# (decisão de 2026-07-09) — a região inteira é excluída da comparação.
FIDUCIAL_BOXES = [(25.6, 119.53), (554.81, 119.53), (26.12, 792.73), (554.29, 792.73)]
EXAMPLE_BOX = (395.0, 306.0, 476.5, 336.0)  # x0, top, x1, bottom
QR_BOX = (506.0, 39.5, 562.0, 95.5)
MATRICULA_BOX = (423.0, 118.0, 572.0, 295.9)


def snapshot(pdf_path: Path) -> dict:
    out = subprocess.run(
        [sys.executable, str(HERE / "snapshot.py"), str(pdf_path)],
        capture_output=True, text=True, check=True,
    )
    return json.loads(out.stdout)["pages"][0]


def in_box(x, y, box):
    return box[0] <= x <= box[2] and box[1] <= y <= box[3]


def is_intentional(x, y):
    if in_box(x, y, EXAMPLE_BOX) or in_box(x, y, QR_BOX) or in_box(x, y, MATRICULA_BOX):
        return True
    for fx, fy in FIDUCIAL_BOXES:
        if fx - 1 <= x <= fx + 16.6 and fy - 1 <= y <= fy + 16.6:
            return True
    return False


def match_texts(ref, cand, tol):
    """Casa text_runs por texto normalizado + proximidade; devolve divergências."""
    from collections import defaultdict

    def norm(t):
        # Ligaduras são artefato de extração (ToUnicode do Chromium mapeia o
        # glifo ﬁ para "fi"; o subsetter do prova-pdf mapeia para U+FB01).
        return t["text"].strip().replace("ﬁ", "fi").replace("ﬂ", "fl")

    cand_by_text = defaultdict(list)
    for c in cand:
        cand_by_text[norm(c)].append(c)

    diverg, removed = [], []
    for r in ref:
        pool = cand_by_text.get(norm(r))
        if not pool:
            removed.append(r)
            continue
        best = min(pool, key=lambda c: abs(c["top"] - r["top"]) + abs(c["x0"] - r["x0"]))
        pool.remove(best)
        dx, dy = best["x0"] - r["x0"], best["top"] - r["top"]
        ds = best["size"] - r["size"]
        if abs(dx) > tol or abs(dy) > tol or abs(ds) > 0.1:
            diverg.append((r, best, dx, dy, ds))

    added = [c for pool in cand_by_text.values() for c in pool]
    return diverg, added, removed


def match_boxes(ref, cand, tol, key=lambda e: (e["x0"], e["top"], e["w"], e["h"])):
    """Casa retângulos/curvas por posição+tamanho com tolerância."""
    cand_left = list(cand)
    diverg, removed = [], []
    for r in ref:
        rx, ry, rw, rh = key(r)
        best, best_d = None, None
        for c in cand_left:
            cx, cy, cw, ch = key(c)
            if abs(cw - rw) > tol * 2 or abs(ch - rh) > tol * 2:
                continue
            d = abs(cx - rx) + abs(cy - ry)
            if best is None or d < best_d:
                best, best_d = c, d
        if best is None:
            removed.append(r)
            continue
        cand_left.remove(best)
        cx, cy, cw, ch = key(best)
        if abs(cx - rx) > tol or abs(cy - ry) > tol:
            diverg.append((r, best, cx - rx, cy - ry))
    return diverg, cand_left, removed


def fmt_text(t):
    return f"y={t['top']:7.2f} x={t['x0']:7.2f} size={t['size']} {t['text'][:50]!r}"


def fmt_box(b):
    return f"({b['x0']:.2f},{b['top']:.2f}) {b['w']:.2f}x{b['h']:.2f}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tolerance-pt", type=float, default=0.5)
    ap.add_argument("--candidate", default=str(HERE / "out/candidate.pdf"))
    args = ap.parse_args()
    tol = args.tolerance_pt

    ref = snapshot(HERE / "reference/folha_respostas.pdf")
    cand = snapshot(Path(args.candidate))

    report = []
    issues = 0

    # ── Text runs ─────────────────────────────────────────────────────────
    ref_t = [t for t in ref["text_runs"] if not is_intentional(t["x0"], t["top"])]
    cand_t = [t for t in cand["text_runs"] if not is_intentional(t["x0"], t["top"])]
    tdiv, tadd, trem = match_texts(ref_t, cand_t, tol)
    report.append(f"## Text runs: {len(tdiv)} divergentes, {len(tadd)} adicionados, {len(trem)} removidos")
    for r, c, dx, dy, ds in tdiv[:40]:
        report.append(f"- Δx={dx:+.2f} Δy={dy:+.2f} Δsize={ds:+.2f}  ref: {fmt_text(r)}")
    for t in trem[:20]:
        report.append(f"- REMOVIDO: {fmt_text(t)}")
    for t in tadd[:20]:
        report.append(f"- ADICIONADO: {fmt_text(t)}")
    issues += len(tdiv) + len(tadd) + len(trem)

    # ── Rects (bordas, sombreamentos, separadores; exclui QR e intencionais) ──
    def rect_ok(r):
        return not is_intentional(r["x0"], r["top"])
    ref_r = [r for r in ref["rects"] if rect_ok(r)]
    cand_r = [r for r in cand["rects"] if rect_ok(r)]
    rdiv, radd, rrem = match_boxes(ref_r, cand_r, tol)
    report.append(f"\n## Rects: {len(rdiv)} divergentes, {len(radd)} adicionados, {len(rrem)} removidos")
    for r, c, dx, dy in rdiv[:40]:
        report.append(f"- Δx={dx:+.2f} Δy={dy:+.2f}  ref: {fmt_box(r)}")
    for r in rrem[:20]:
        report.append(f"- REMOVIDO: {fmt_box(r)}")
    for r in radd[:20]:
        report.append(f"- ADICIONADO: {fmt_box(r)}")
    issues += len(rdiv) + len(radd) + len(rrem)

    # ── Curves (bolhas) ───────────────────────────────────────────────────
    ref_c = [c for c in ref["curves"] if not is_intentional(c["x0"], c["top"])]
    cand_c = [c for c in cand["curves"] if not is_intentional(c["x0"], c["top"])]
    cdiv, cadd, crem = match_boxes(ref_c, cand_c, tol)
    report.append(f"\n## Curves (bolhas): {len(cdiv)} divergentes, {len(cadd)} adicionadas, {len(crem)} removidas")
    for r, c, dx, dy in cdiv[:40]:
        report.append(f"- Δx={dx:+.2f} Δy={dy:+.2f}  ref: {fmt_box(r)}")
    for c in crem[:10]:
        report.append(f"- REMOVIDA: {fmt_box(c)}")
    for c in cadd[:10]:
        report.append(f"- ADICIONADA: {fmt_box(c)}")
    issues += len(cdiv) + len(cadd) + len(crem)

    # ── QR: compara apenas o bounding box do cluster de módulos ──────────
    def qr_bbox(snap):
        mods = [r for r in snap["rects"] if r["w"] < 2.5 and r["h"] < 2.5
                and in_box(r["x0"], r["top"], QR_BOX)]
        if not mods:
            return None
        return (min(m["x0"] for m in mods), min(m["top"] for m in mods),
                max(m["x1"] for m in mods), max(m["bottom"] for m in mods))
    rq, cq = qr_bbox(ref), qr_bbox(cand)
    report.append("\n## QR")
    if rq and cq:
        deltas = [abs(a - b) for a, b in zip(rq, cq)]
        ok = all(d <= tol for d in deltas)
        report.append(f"- bbox ref {tuple(round(v,2) for v in rq)} vs cand {tuple(round(v,2) for v in cq)} → {'OK' if ok else 'DIVERGENTE'}")
        if not ok:
            issues += 1
    else:
        report.append(f"- ref presente: {bool(rq)}, cand presente: {bool(cq)}")
        issues += rq is None or cq is None

    # ── Logo (única imagem esperada no candidato) ─────────────────────────
    ref_logo = [i for i in ref["images"] if i["w"] > 50]
    cand_img = cand["images"]
    report.append("\n## Imagens")
    if ref_logo and cand_img:
        r, c = ref_logo[0], cand_img[0]
        dx, dy = c["x0"] - r["x0"], c["top"] - r["top"]
        dw, dh = c["w"] - r["w"], c["h"] - r["h"]
        ok = all(abs(v) <= tol for v in (dx, dy, dw, dh))
        report.append(f"- logo Δx={dx:+.2f} Δy={dy:+.2f} Δw={dw:+.2f} Δh={dh:+.2f} → {'OK' if ok else 'DIVERGENTE'}")
        if not ok:
            issues += 1
    else:
        report.append(f"- logo: ref={len(ref_logo)}, cand={len(cand_img)}")

    text = "\n".join(report)
    (HERE / "out/diff.md").write_text(text + "\n")
    print(text)
    print(f"\nTotal de problemas: {issues} (tolerância {tol}pt)")
    sys.exit(0 if issues == 0 else 1)


if __name__ == "__main__":
    main()
