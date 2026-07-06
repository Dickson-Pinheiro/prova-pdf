"""
Exam formatter — converts raw database rows + HTML into prova-pdf ExamSpec structures.

This module contains NO database access. It receives plain dicts/lists and
produces ExamSpec-compatible JSON structures. Designed for reuse in the lize
Django project.

Public API:
    html_to_inline(html_str, images) → list[dict]
    build_print_config(exam_row)     → dict
    build_question(...)              → dict
    build_base_text(bt_row, ...)     → dict
    build_answer(q_row, alts)        → dict
    ImageRegistry(images_dir)        — downloads and tracks images
    StyleAccum                       — style state during HTML walk
"""

from __future__ import annotations

import io
import re
import sys
from copy import copy
from dataclasses import dataclass
from pathlib import Path
from typing import Optional
from urllib.parse import urlparse

from bs4 import BeautifulSoup, Comment, NavigableString, Tag
from PIL import Image

# ── Constants ─────────────────────────────────────────────────────────────────

FONT_SIZE_MAP = {0: 12, 1: 14, 2: 18, 3: 22, 4: 32, 5: 8, 6: 10, 7: 11}
LINE_SPACING_MAP = {0: "normal", 1: "oneAndHalf", 2: "twoAndHalf", 3: "threeAndHalf"}
DISCURSIVE_SPACE_MAP = {0: "lines", 1: "blank", 2: "lines"}
BASE_TEXT_POSITION_MAP = {0: "beforeQuestion", 1: "sectionTop", 2: "examTop", 3: "examBottom"}
CATEGORY_KIND_MAP = {0: "textual", 1: "choice", 2: "file", 3: "sum", 4: "cloze"}

FONT_FAMILY_MAP = {0: "body", 1: "verdana", 2: "times", 3: "arial", 4: "nunito"}
SEPARATION_MODE_MAP = {0: "single", 1: "perSubject", 2: "perCategory"}
LANGUAGE_MAP = {0: "pt", 1: "en", 2: "es"}

FONT_SIZE_MAP_NEW = {0: 15, 1: 17, 2: 23, 3: 26, 4: 38, 5: 10, 6: 12, 7: 13}

SEDUC_CLIENT_PK = "83579d53-7d1c-477d-aafc-09c3070bdb41"
DECISAO_CLIENT_PK = "a2b1158b-367a-40a4-8413-9897057c8aa2"
FONT_SIZE_NEW_CUTOFF_DATE = "2024-07-07"
DECISAO_OLD_FONT_CUTOFF_DATE = "2024-12-31"

BLOCK_TAGS = {"p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "li", "blockquote", "section"}

# VML block without HTML comment wrapper: <![if gte vml N]>...<![endif]>
_RE_VML_BLOCK = re.compile(
    r"<!\[if\s+gte\s+vml[^\]]*\]>.*?<!\[endif\]>",
    re.DOTALL | re.IGNORECASE,
)
# VML block with HTML comment wrapper: <!-- [if gte vml N]>...<![endif]-->
_RE_VML_BLOCK_COMMENT = re.compile(
    r"<!--\s*\[if\s+gte\s+vml[^\]]*\]>.*?<!\[endif\]\s*-->",
    re.DOTALL | re.IGNORECASE,
)
# Word supportFields blocks (field codes): <!-- [if supportFields]>...<![endif]-->
_RE_SUPPORT_FIELDS = re.compile(
    r"<!--\s*\[if\s+supportFields\]>.*?<!\[endif\]\s*-->",
    re.DOTALL | re.IGNORECASE,
)
# [if !vml] without comment wrapper — unwrap to keep fallback content
_RE_IFVML_WRAP = re.compile(
    r"<!\[if\s+!vml\]>(.*?)<!\[endif\]>",
    re.DOTALL | re.IGNORECASE,
)
# Splits a text string by LaTeX math delimiters: \(...\), \[...\], $$...$$, $...$
_RE_MATH = re.compile(
    r"(\\\[.*?\\\]|\\\(.*?\\\)|\$\$.*?\$\$|\$(?!\$).*?(?<!\$)\$)",
    re.DOTALL,
)

# ── Style accumulator ─────────────────────────────────────────────────────────

@dataclass
class StyleAccum:
    bold: bool = False
    italic: bool = False
    underline: bool = False
    color: Optional[str] = None
    font_size: Optional[float] = None
    text_align: Optional[str] = None  # "center" or "right"

    def copy(self) -> StyleAccum:
        return copy(self)

    def to_dict(self) -> Optional[dict]:
        d: dict = {}
        if self.bold:
            d["bold"] = True
        if self.italic:
            d["italic"] = True
        if self.underline:
            d["underline"] = True
        if self.color:
            d["color"] = self.color
        if self.font_size:
            d["fontSize"] = self.font_size
        if self.text_align:
            d["textAlign"] = self.text_align
        return d if d else None


# ── Image registry ────────────────────────────────────────────────────────────

class ImageRegistry:
    """Downloads, caches, and tracks images referenced in exam HTML."""

    def __init__(self, images_dir: Path):
        self.images_dir = images_dir
        self.images_dir.mkdir(parents=True, exist_ok=True)
        self._url_to_key: dict[str, str] = {}
        self._key_to_dims: dict[str, tuple[int, int]] = {}
        self.key_to_path: dict[str, str] = {}
        self._counter = 0
        # Base directory for computing relative paths.  Caller can override.
        self.base_dir: Path = images_dir.parent.parent.parent

    def register(self, url: str, width_px: int, height_px: int) -> dict:
        if url.startswith("data:"):
            url = self._save_data_uri(url)
            if url is None:
                return {"type": "text", "value": ""}
        if url in self._url_to_key:
            key = self._url_to_key[url]
            width_px, height_px = self._key_to_dims.get(key, (width_px, height_px))
        else:
            self._counter += 1
            key = f"img_{self._counter}"
            ext = self._guess_ext(url)
            filepath = self.images_dir / f"{key}{ext}"
            self._download(url, filepath)
            if not width_px or not height_px:
                actual = self._read_dims(filepath)
                if actual:
                    aw, ah = actual
                    if not width_px:
                        width_px = aw
                    if not height_px:
                        height_px = ah
            self._url_to_key[url] = key
            self._key_to_dims[key] = (width_px, height_px)
            self.key_to_path[key] = str(filepath.relative_to(self.base_dir))

        node: dict = {"type": "image", "key": key}
        if width_px:
            node["widthCm"] = round(width_px / 96 * 2.54, 2)
        if height_px:
            node["heightCm"] = round(height_px / 96 * 2.54, 2)
        return node

    @staticmethod
    def _read_dims(filepath: Path) -> Optional[tuple[int, int]]:
        try:
            img = Image.open(filepath)
            return img.size
        except Exception:
            return None

    def _save_data_uri(self, data_uri: str) -> Optional[str]:
        import base64
        try:
            header, encoded = data_uri.split(",", 1)
            mime = header.split(";")[0].split(":")[1]
            ext = {"image/png": ".png", "image/jpeg": ".jpg", "image/gif": ".gif"}.get(mime, ".png")
            self._counter += 1
            key = f"img_{self._counter}"
            filepath = self.images_dir / f"{key}{ext}"
            filepath.write_bytes(base64.b64decode(encoded))
            print(f"  [img] decoded data URI → {filepath.name}", file=sys.stderr)
            self._url_to_key[data_uri] = key
            dims = self._read_dims(filepath)
            w, h = dims if dims else (0, 0)
            self._key_to_dims[key] = (w, h)
            self.key_to_path[key] = str(filepath.relative_to(self.base_dir))
            return data_uri
        except Exception as exc:
            print(f"  [img] WARN: failed to decode data URI: {exc}", file=sys.stderr)
            return None

    def register_as(self, url: str, key: str) -> bool:
        """Download url and register it under an explicit key. Returns True on success."""
        if url in self._url_to_key:
            old_key = self._url_to_key[url]
            if old_key != key:
                # Re-alias under the requested key
                self.key_to_path[key] = self.key_to_path.get(old_key, "")
                self._url_to_key[url] = key
            return True
        ext = self._guess_ext(url)
        filepath = self.images_dir / f"{key}{ext}"
        self._download(url, filepath)
        if not filepath.exists():
            return False
        self._url_to_key[url] = key
        dims = self._read_dims(filepath)
        self._key_to_dims[key] = dims if dims else (0, 0)
        self.key_to_path[key] = str(filepath.relative_to(self.base_dir))
        return True

    @staticmethod
    def _guess_ext(url: str) -> str:
        path = urlparse(url).path.lower()
        for ext in (".png", ".jpg", ".jpeg", ".gif", ".svg"):
            if path.endswith(ext):
                return ".jpg" if ext == ".jpeg" else ext
        return ".png"

    @staticmethod
    def _download(url: str, dest: Path) -> None:
        import requests
        try:
            r = requests.get(url, timeout=20)
            r.raise_for_status()
            content = r.content
            if content[:4] == b'RIFF' and content[8:12] == b'WEBP':
                img = Image.open(io.BytesIO(content))
                dest = dest.with_suffix('.png')
                buf = io.BytesIO()
                img.save(buf, 'PNG')
                dest.write_bytes(buf.getvalue())
                print(f"  [img] downloaded+converted WebP→PNG {dest.name}", file=sys.stderr)
            else:
                dest.write_bytes(content)
                print(f"  [img] downloaded {dest.name}", file=sys.stderr)
        except Exception as exc:
            print(f"  [img] WARN: failed to download {url}: {exc}", file=sys.stderr)


# ── CSS style parser ──────────────────────────────────────────────────────────

def _parse_css_style(style_attr: str) -> dict:
    props: dict = {}
    for decl in style_attr.split(";"):
        if ":" not in decl:
            continue
        key, _, val = decl.partition(":")
        key = key.strip().lower()
        val = val.strip()
        if key == "color":
            c = _normalize_color(val)
            if c:
                props["color"] = c
        elif key == "font-size":
            pt = _parse_pt(val)
            if pt:
                props["font_size"] = pt
        elif key == "font-weight" and val in ("bold", "700", "800", "900"):
            props["bold"] = True
        elif key == "font-style" and val == "italic":
            props["italic"] = True
        elif key == "text-decoration" and "underline" in val:
            props["underline"] = True
        elif key == "text-align" and val in ("center", "right", "left", "justify"):
            props["text_align"] = val
        elif key == "margin-left":
            pt = _parse_pt(val)
            if pt:
                props["margin_left_pt"] = pt
    return props


def _normalize_color(val: str) -> Optional[str]:
    val = val.strip()
    if val.startswith("#") and len(val) in (4, 7):
        if len(val) == 4:
            val = "#" + "".join(c * 2 for c in val[1:])
        return val.upper()
    m = re.match(r"rgb\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)", val)
    if m:
        return "#{:02X}{:02X}{:02X}".format(int(m.group(1)), int(m.group(2)), int(m.group(3)))
    return None


def _parse_pt(val: str) -> Optional[float]:
    m = re.match(r"([\d.]+)\s*(pt|px|em|rem)?", val)
    if not m:
        return None
    num = float(m.group(1))
    unit = (m.group(2) or "pt").lower()
    if unit == "px":
        return round(num * 0.75, 1)
    if unit in ("em", "rem"):
        return round(num * 12, 1)
    return round(num, 1)


# ── HTML → InlineContent walker ───────────────────────────────────────────────

def _emit_text_or_math(raw: str, style: StyleAccum, out: list) -> None:
    """Split a plain string by LaTeX delimiters and emit text/math nodes."""
    for part in _RE_MATH.split(raw):
        if not part:
            continue
        if part.startswith(r"\(") and part.endswith(r"\)"):
            out.append({"type": "math", "latex": part[2:-2].strip(), "display": False})
        elif part.startswith(r"\[") and part.endswith(r"\]"):
            out.append({"type": "math", "latex": part[2:-2].strip(), "display": True})
        elif part.startswith("$$") and part.endswith("$$"):
            out.append({"type": "math", "latex": part[2:-2].strip(), "display": True})
        elif part.startswith("$") and part.endswith("$"):
            out.append({"type": "math", "latex": part[1:-1].strip(), "display": False})
        else:
            text = part.replace("\xa0", " ")
            if text:
                item: dict = {"type": "text", "value": text}
                sd = style.to_dict()
                if sd:
                    item["style"] = sd
                out.append(item)


def _walk_node(node, style: StyleAccum, out: list, images: ImageRegistry, compact: bool = False) -> None:
    if isinstance(node, Comment):
        return

    if isinstance(node, NavigableString):
        raw = str(node).replace("\xa0", " ")
        # Strip source-level newlines/tabs (HTML whitespace between block elements).
        # Spaces are preserved since they may separate inline words.
        raw = raw.strip("\n\t\r")
        if raw:
            _emit_text_or_math(raw, style, out)
        return

    if not isinstance(node, Tag):
        return

    tag = node.name.lower() if node.name else ""

    if tag == "span" and "mce-nbsp-wrap" in (node.get("class") or []):
        return

    if tag == "math":
        display = node.get("display") == "block"
        annotation = node.find("annotation", {"encoding": "application/x-tex"})
        if annotation:
            latex = annotation.get_text().strip()
            if latex:
                out.append({"type": "math", "latex": latex, "display": display})
                return
        text = node.get_text().strip()
        if text:
            out.append({"type": "text", "value": text})
        return

    if tag == "br":
        out.append({"type": "text", "value": "\n"})
        return

    if tag == "img":
        src = node.get("src", "")
        if src:
            try:
                w = int(node.get("width", 0) or 0)
                h = int(node.get("height", 0) or 0)
            except (ValueError, TypeError):
                w, h = 0, 0
            out.append(images.register(src, w, h))
        return

    if tag in ("b", "strong"):
        ns = style.copy()
        ns.bold = True
        _walk_children(node, ns, out, images, compact)
        return

    if tag in ("i", "em"):
        ns = style.copy()
        ns.italic = True
        _walk_children(node, ns, out, images, compact)
        return

    if tag == "u":
        ns = style.copy()
        ns.underline = True
        _walk_children(node, ns, out, images, compact)
        return

    if tag == "sub":
        inner: list = []
        _walk_children(node, style.copy(), inner, images, compact)
        inner = _collapse_text(inner)
        if inner:
            if all(i.get("type") == "image" for i in inner):
                out.extend(inner)
            else:
                out.append({"type": "sub", "content": inner})
        return

    if tag == "sup":
        inner = []
        _walk_children(node, style.copy(), inner, images, compact)
        inner = _collapse_text(inner)
        if inner:
            if all(i.get("type") == "image" for i in inner):
                out.extend(inner)
            else:
                out.append({"type": "sup", "content": inner})
        return

    if tag == "span":
        ns = style.copy()
        css = _parse_css_style(node.get("style", ""))
        if css.get("bold"):
            ns.bold = True
        if css.get("italic"):
            ns.italic = True
        if css.get("underline"):
            ns.underline = True
        if css.get("color"):
            ns.color = css["color"]
        if css.get("font_size"):
            ns.font_size = css["font_size"]
        _walk_children(node, ns, out, images, compact)
        return

    if tag in BLOCK_TAGS:
        ns = style.copy()
        css = _parse_css_style(node.get("style", ""))
        margin_left = css.get("margin_left_pt", 0) or 0
        text_align = css.get("text_align", "")
        html_align = (node.get("align") or "").lower()
        if text_align == "center" or html_align == "center" or margin_left > 30:
            ns.text_align = "center"
        elif text_align == "right" or html_align == "right":
            ns.text_align = "right"
        sep = "\n" if compact else "\n\n"
        if out and not _ends_with_separator(out):
            out.append({"type": "text", "value": sep})
        _walk_children(node, ns, out, images, compact)
        if out and not _ends_with_separator(out):
            out.append({"type": "text", "value": sep})
        return

    if tag in ("ul", "ol"):
        _walk_children(node, style, out, images, compact)
        return

    if tag == "a":
        _walk_children(node, style, out, images, compact)
        return

    if tag == "table":
        _walk_table(node, style, out, images, compact)
        return

    if tag in ("tbody", "thead"):
        _walk_children(node, style, out, images, compact)
        return

    if tag in ("tr", "td", "th"):
        # Handled inside _walk_table; if reached standalone, fall through to children.
        _walk_children(node, style, out, images, compact)
        return

    _walk_children(node, style, out, images, compact)


def _walk_table(node: Tag, style: StyleAccum, out: list, images: ImageRegistry, compact: bool) -> None:
    """Parse HTML table: cells in the same <tr> are joined with a space,
    rows are separated by a single newline. This produces compact inline text
    matching Chromium's table rendering (cells side-by-side, rows stacked)."""
    rows = node.find_all("tr")
    if not rows:
        _walk_children(node, style, out, images, compact)
        return

    sep = "\n" if compact else "\n\n"
    if out and not _ends_with_separator(out):
        out.append({"type": "text", "value": sep})

    for row_idx, tr in enumerate(rows):
        cells = tr.find_all(["td", "th"])
        if not cells:
            continue

        for cell_idx, td in enumerate(cells):
            if cell_idx > 0:
                out.append({"type": "text", "value": " "})
            # Walk cell content in compact mode to suppress block-level separators.
            cell_out: list = []
            _walk_children(td, style, cell_out, images, compact=True)
            cell_out = _collapse_text(cell_out)
            out.extend(cell_out)

        # Separate rows with a single newline (not \n\n).
        if row_idx < len(rows) - 1:
            out.append({"type": "text", "value": "\n"})

    if out and not _ends_with_separator(out):
        out.append({"type": "text", "value": sep})


def _walk_children(node: Tag, style: StyleAccum, out: list, images: ImageRegistry, compact: bool = False) -> None:
    for child in node.children:
        _walk_node(child, style, out, images, compact)


def _ends_with_separator(out: list) -> bool:
    return (
        out
        and out[-1].get("type") == "text"
        and out[-1].get("value", "").strip() == ""
    )


def _collapse_text(items: list) -> list:
    """Merge adjacent text nodes with identical styles; strip leading/trailing whitespace-only."""
    result: list = []
    for item in items:
        if (
            item["type"] == "text"
            and result
            and result[-1]["type"] == "text"
            and result[-1].get("style") == item.get("style")
        ):
            result[-1]["value"] += item["value"]
        else:
            result.append(item)

    while result and result[0].get("type") == "text" and not result[0]["value"].strip():
        result.pop(0)
    while result and result[-1].get("type") == "text" and not result[-1]["value"].strip():
        result.pop()

    for item in result:
        if item.get("type") == "text":
            # Allow up to 3 newlines (\n\n\n = paragraph break with leading <br>).
            # 4+ newlines are collapsed to \n\n\n to prevent runaway whitespace.
            item["value"] = re.sub(r"\n{4,}", "\n\n\n", item["value"])

    if result and result[-1].get("type") == "text":
        result[-1]["value"] = result[-1]["value"].rstrip("\n")
        if not result[-1]["value"]:
            result.pop()

    if result and result[0].get("type") == "text":
        result[0]["value"] = result[0]["value"].lstrip("\n")
        if not result[0]["value"]:
            result.pop(0)

    def _is_block_image(node: dict) -> bool:
        return node.get("type") == "image" and node.get("heightCm", 0) > 1.5

    for i, item in enumerate(result):
        if item.get("type") == "text":
            if i > 0 and _is_block_image(result[i - 1]):
                item["value"] = re.sub(r"^[ \t]+", "", item["value"])
            if i < len(result) - 1 and _is_block_image(result[i + 1]):
                item["value"] = re.sub(r"[ \t]+$", "", item["value"])

    result = [item for item in result if not (item.get("type") == "text" and not item["value"])]
    return result


def _strip_vml(html: str) -> str:
    """Remove VML/Word conditional blocks and unwrap [if !vml] fallback content."""
    html = _RE_VML_BLOCK_COMMENT.sub("", html)
    html = _RE_SUPPORT_FIELDS.sub("", html)
    html = _RE_VML_BLOCK.sub("", html)
    html = _RE_IFVML_WRAP.sub(r"\1", html)
    return html


def html_to_inline(html_str: str, images: ImageRegistry, compact: bool = False) -> list:
    """Convert an HTML string to a list of prova-pdf InlineContent dicts.

    compact=True uses single newlines instead of double newlines for block
    separators, producing compact inline content suitable for alternatives.
    """
    if not html_str:
        return []
    html_str = _strip_vml(html_str)
    soup = BeautifulSoup(html_str, "lxml")
    body = soup.find("body") or soup
    out: list = []
    _walk_children(body, StyleAccum(), out, images, compact)
    return _collapse_text(out)


# ── Helpers ───────────────────────────────────────────────────────────────────

_CHROMIUM_DPI_SCALE = 72 / 96  # CSS pt → PDF pt via Chromium (renders at 96dpi, outputs 72pt/in)


def get_effective_font_size(font_size_idx: int, client_pk: str, exam_created_at) -> float:
    """Return font size in PDF points, calibrated to match Chromium's rendered output.

    The Django template always uses the new font table (FONT_SIZE_MAP_NEW) and emits
    CSS pt values (e.g. font-size: 15pt). Chromium renders at 96px/in and outputs PDF
    at 72pt/in, applying a scale of 72/96 = 0.75 to all pt values. prova-pdf renders
    at true PDF pt, so we pre-scale to match Chromium's effective output.
    """
    if hasattr(exam_created_at, "strftime"):
        date_str = exam_created_at.strftime("%Y-%m-%d")
    else:
        date_str = str(exam_created_at or "")[:10]

    if client_pk == DECISAO_CLIENT_PK and date_str <= DECISAO_OLD_FONT_CUTOFF_DATE:
        css_pt = FONT_SIZE_MAP.get(font_size_idx, 12)
        return round(css_pt * _CHROMIUM_DPI_SCALE, 2)

    # Template always uses new table regardless of creation date (old branch commented out).
    css_pt = FONT_SIZE_MAP_NEW.get(font_size_idx, 15)
    return round(css_pt * _CHROMIUM_DPI_SCALE, 2)


def _dummy_registry() -> "ImageRegistry":
    reg = ImageRegistry.__new__(ImageRegistry)
    reg._url_to_key = {}
    reg._key_to_dims = {}
    reg.key_to_path = {}
    reg._counter = 0
    reg.images_dir = Path("/tmp")
    reg.base_dir = Path("/tmp")
    return reg


# ── Builders ──────────────────────────────────────────────────────────────────

def build_print_config(exam: dict, overrides: dict = None) -> dict:
    """Build a PrintConfig dict from a merged exam+printconfig database row."""
    if overrides is None:
        overrides = {}

    col = exam.get("column_type") or 0
    fs_idx = exam.get("font_size") or 0
    ls_idx = exam.get("line_height") or 0
    ds_idx = exam.get("discursive_question_space_type") or 0
    ff_idx = exam.get("font_family") or 0

    client_pk = exam.get("client_pk") or ""
    created_at = exam.get("created_at") or "2024-01-01"
    is_seduc = client_pk == SEDUC_CLIENT_PK

    cfg: dict = {
        "columns": 2 if col == 1 else 1,
        "fontSize": get_effective_font_size(fs_idx, client_pk, created_at),
        "fontFamily": FONT_FAMILY_MAP.get(ff_idx, "body"),
        "lineSpacing": LINE_SPACING_MAP.get(ls_idx, "normal"),
        "margins": {
            "top": float(exam.get("margin_top") or 0.6),
            "bottom": float(exam.get("margin_bottom") or 0.6),
            "left": float(exam.get("margin_left") or 0.6),
            "right": float(exam.get("margin_right") or 0.6),
        },
        "discursiveLineHeight": float(exam.get("discursive_line_height") or 0.85),
        "discursiveSpaceType": DISCURSIVE_SPACE_MAP.get(ds_idx, "lines"),
        "separationMode": SEPARATION_MODE_MAP.get(exam.get("kind") or 0, "single"),
        "headerFull": exam.get("header_format") == 0,
        "paperSize": "Ata" if is_seduc else overrides.get("paper_size", "A4"),
        "allBlack": True if is_seduc else overrides.get("all_black", False),
    }

    bool_fields = {
        "print_black_and_white_images": "imageGrayscale",
        "show_question_score": "showScore",
        "economy_mode": "economyMode",
        "hide_numbering": "hideNumbering",
        "hide_knowledge_areas_name": "hideKnowledgeAreaName",
        "hide_questions_referencies": "hideQuestionsReferences",
        "show_question_board": "showQuestionBoard",
        "break_enunciation": "breakEnunciation",
        "break_alternatives": "breakAlternatives",
        "break_all_questions": "breakAllQuestions",
        "remove_color_alternatives": "removeColorAlternatives",
    }
    for db_key, json_key in bool_fields.items():
        if exam.get(db_key):
            cfg[json_key] = True

    if exam.get("force_choices_with_statement"):
        cfg["forceChoicesWithStatement"] = 1

    cfg["letterCase"] = "upper"

    for db_key, json_key in (
        ("hyphenate", "hyphenate"),
        ("show_footer", "showFooter"),
        ("add_page_number", "addPageNumber"),
    ):
        if exam.get(db_key):
            cfg[json_key] = True

    if exam.get("show_footer"):
        cfg["footerText"] = exam.get("name")

    return cfg


def build_url_params(exam: dict) -> dict:
    """Build URL query params for /imprimir that mirror get_filters_to_print() in the Django model.

    Returns a dict suitable for urlencode() to be appended to the Chromium print URL.
    This ensures capture_reference.py generates PDFs with the same settings as production.

    NOTE: font_family is forced to 0 (IBM Plex Sans) because prova-pdf only has IBM Plex Sans
    registered and falls back to "body" when other families aren't registered. Chromium must use
    the same font for a valid structural comparison.
    """
    economy_mode = bool(exam.get("economy_mode"))
    return {
        "header_full": int(exam.get("header_format") or 0),
        "two_columns": 1 if economy_mode else int(exam.get("column_type") or 0),
        "separate_subjects": int(exam.get("kind") or 0),
        "line_spacing": int(exam.get("line_height") or 0),
        "font_size": int(exam.get("font_size") or 0),
        "font_family": 0,  # forced: prova-pdf falls back to "body" (IBM Plex Sans)
        "hide_discipline_name": 0 if not exam.get("print_subjects_name") else 1,
        "hide_knowledge_area_name": int(bool(exam.get("hide_knowledge_areas_name"))),
        "hide_questions_referencies": int(bool(exam.get("hide_questions_referencies"))),
        "print_images_with_grayscale": int(bool(exam.get("print_black_and_white_images"))),
        "hyphenate_text": int(bool(exam.get("hyphenate"))),
        "show_question_score": int(bool(exam.get("show_question_score"))),
        "show_question_board": int(bool(exam.get("show_question_board"))),
        "margin_top": float(exam.get("margin_top") or 0.6),
        "margin_bottom": float(exam.get("margin_bottom") or 0.6),
        "margin_right": float(exam.get("margin_right") or 0.0),
        "margin_left": float(exam.get("margin_left") or 0.0),
        "uppercase_letters": int(bool(exam.get("uppercase_letters"))),
        "discursive_line_height": float(exam.get("discursive_line_height") or 1.0),
        "show_footer": int(bool(exam.get("show_footer"))),
        "add_page_number": int(bool(exam.get("add_page_number"))),
        "economy_mode": int(economy_mode),
        "force_choices_with_statement": int(bool(exam.get("force_choices_with_statement"))),
        "hide_numbering": int(bool(exam.get("hide_numbering"))),
        "break_enunciation": 1 if economy_mode else int(bool(exam.get("break_enunciation"))),
        "break_all_questions": int(bool(exam.get("break_all_questions"))),
        "remove_color_alternatives": int(bool(exam.get("remove_color_alternatives"))),
        "discursive_question_space_type": int(exam.get("discursive_question_space_type") or 0),
        "break_alternatives": int(bool(exam.get("break_alternatives"))),
        "pass_check_can_print": "true",
    }


def build_answer(q: dict, alts: list) -> dict:
    """Build an AnswerSpace dict from a question row and its alternatives."""
    kind = "essay" if q["is_essay"] else CATEGORY_KIND_MAP.get(q["category"], "textual")

    if kind == "choice":
        alternatives = []
        for i, alt in enumerate(sorted(alts, key=lambda a: a["index"])):
            label = chr(ord("A") + i)
            alternatives.append({
                "label": label,
                "content": [],
                "_raw_text": alt["text"],
            })
        return {"type": "choice", "_alternatives_raw": alternatives}

    if kind == "textual":
        ans: dict = {"type": "textual"}
        lines = q.get("quantity_lines")
        if lines:
            ans["lineCount"] = int(lines)
        return ans

    if kind == "essay":
        ans = {"type": "essay"}
        lines = q.get("quantity_lines")
        if lines:
            ans["lineCount"] = int(lines)
        return ans

    if kind == "file":
        # For printed exams, file questions show answer lines (same as essay).
        # lize template: v-if="!['Objetiva', 'Somatório'].includes(question.category)"
        # includes 'Arquivo' (file) in the answer-lines block.
        ans = {"type": "essay"}
        lines = q.get("quantity_lines")
        if lines:
            ans["lineCount"] = int(lines)
        return ans

    if kind == "sum":
        items = []
        for i, alt in enumerate(sorted(alts, key=lambda a: a["index"])):
            label = str(2 ** i)
            items.append({
                "label": label,
                "content": [],
                "_raw_text": alt["text"],
            })
        return {"type": "sum", "_items_raw": items}

    if kind == "cloze":
        _dummy = _dummy_registry()
        word_bank = [html_to_inline(alt["text"], _dummy)
                     for alt in sorted(alts, key=lambda a: a["index"])]
        cloze_content = q.get("cloze_content") or ""
        return {"type": "cloze", "wordBank": word_bank, "_cloze_content": cloze_content}

    return {"type": "textual"}


def build_base_text(bt_row: dict, position: str, images: ImageRegistry) -> dict:
    """Build a BaseText dict from a base_text database row."""
    content = html_to_inline(bt_row["text"], images)
    bt: dict = {
        "content": content,
        "position": position,
    }
    title = (bt_row.get("title") or "").strip()
    if title:
        bt["title"] = title
    return bt


def build_question(
    q: dict,
    alts: list,
    base_texts: list,
    bt_position: str,
    images: ImageRegistry,
    number: int,
) -> dict:
    """Build a Question dict from a question row, its alternatives, and base texts."""
    kind = "essay" if q["is_essay"] else CATEGORY_KIND_MAP.get(q["category"], "textual")
    # For printed exams, file questions show answer lines (same as essay).
    if kind == "file":
        kind = "essay"

    stem = html_to_inline(q["enunciation"], images)

    answer = build_answer(q, alts)
    if answer.get("_alternatives_raw"):
        parsed_alts = []
        for raw in answer.pop("_alternatives_raw"):
            parsed_alts.append({
                "label": raw["label"],
                "content": html_to_inline(raw["_raw_text"], images, compact=True),
            })
        answer["alternatives"] = parsed_alts
        answer.pop("_alternatives_raw", None)

    if answer.get("_items_raw"):
        parsed_items = []
        for raw in answer.pop("_items_raw"):
            parsed_items.append({
                "label": raw["label"],
                "content": html_to_inline(raw["_raw_text"], images, compact=True),
            })
        answer["items"] = parsed_items

    bt_list = []
    for bt_row in base_texts:
        bt_list.append(build_base_text(bt_row, bt_position, images))

    question: dict = {
        "kind": kind,
        "number": number,
        "stem": stem,
        "answer": answer,
    }

    if bt_list:
        question["baseTexts"] = bt_list

    weight = q.get("weight")
    if weight is not None:
        question["points"] = float(weight)

    if q.get("force_one_column"):
        question["fullWidth"] = True

    draft = q.get("draft_rows_number") or 0
    if draft:
        question["draftLines"] = int(draft)

    if q.get("number_is_hidden"):
        question["showNumber"] = False

    if q.get("force_break_page"):
        question["forcePageBreak"] = True

    if q.get("board"):
        question["board"] = q["board"]

    if q.get("level") is not None:
        question["level"] = q["level"]

    if q.get("theme"):
        question["theme"] = q["theme"]

    if q.get("break_enunciation"):
        question["breakEnunciation"] = True

    if q.get("break_alternatives"):
        question["breakAlternatives"] = True

    if q.get("force_choices_with_statement"):
        question["forceChoicesWithStatement"] = True

    if q.get("print_only_enunciation"):
        question["printOnlyEnunciation"] = True

    if q.get("support_content_question"):
        question["supportContent"] = html_to_inline(q["support_content_question"], images)
        if q.get("support_content_position"):
            question["supportContentPosition"] = q["support_content_position"]

    return question


def build_header(exam: dict, client: dict, exam_header: dict = None) -> dict:
    """Build a Header dict from exam row, client data, and optional ExamHeader row.

    Student fields match the Django template logic (not_separate.html / separate_subjects.html):
    - separate_subjects == 1 (separate_subjects.html): always shows full header fields
    - header_format == 1  (not_separate.html, header_full URL param = 1): full header
    - otherwise: only ALUNO row

    Full header = ALUNO (full row) + [Nº, SÉRIE, TURMA, TURNO] (shared row) + PROFESSOR (full row).
    """
    # Determine whether to show full student fields.
    # header_format == 1 → URL param header_full=1 → Django template shows full header.
    # separate_subjects == 1 → separate_subjects.html always shows full fields.
    header_format = int(exam.get("header_format") or 0)
    separate_subjects = int(exam.get("kind") or 0)
    show_full = (header_format == 1) or (separate_subjects == 1)

    if show_full:
        # Matches not_separate.html {% if header_full %} / separate_subjects.html
        student_fields = [
            {"label": "Aluno"},
            {"label": "Nº", "widthCm": 1},
            {"label": "Série", "widthCm": 1},
            {"label": "Turma", "widthCm": 1},
            {"label": "Turno", "widthCm": 1},
            {"label": "Professor"},
        ]
    else:
        # Simple header: only ALUNO row (not_separate.html without header_full)
        student_fields = [
            {"label": "Aluno"},
        ]

    header: dict = {
        "title": exam.get("name", ""),
        "studentFields": student_fields,
    }

    if client.get("client_name"):
        header["institution"] = client["client_name"]

    if client.get("client_logo"):
        header["logoKey"] = "client_logo"

    if exam.get("external_code"):
        header["externalCode"] = str(exam["external_code"])

    if exam_header and exam_header.get("content"):
        header["customContent"] = exam_header["content"]

    return header
