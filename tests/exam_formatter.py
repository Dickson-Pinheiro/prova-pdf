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


def _walk_node(node, style: StyleAccum, out: list, images: ImageRegistry) -> None:
    if isinstance(node, Comment):
        return

    if isinstance(node, NavigableString):
        raw = str(node).replace("\xa0", " ")
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
        _walk_children(node, ns, out, images)
        return

    if tag in ("i", "em"):
        ns = style.copy()
        ns.italic = True
        _walk_children(node, ns, out, images)
        return

    if tag == "u":
        ns = style.copy()
        ns.underline = True
        _walk_children(node, ns, out, images)
        return

    if tag == "sub":
        inner: list = []
        _walk_children(node, style.copy(), inner, images)
        inner = _collapse_text(inner)
        if inner:
            if all(i.get("type") == "image" for i in inner):
                out.extend(inner)
            else:
                out.append({"type": "sub", "content": inner})
        return

    if tag == "sup":
        inner = []
        _walk_children(node, style.copy(), inner, images)
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
        _walk_children(node, ns, out, images)
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
        if out and not _ends_with_separator(out):
            out.append({"type": "text", "value": "\n\n"})
        _walk_children(node, ns, out, images)
        if out and not _ends_with_separator(out):
            out.append({"type": "text", "value": "\n\n"})
        return

    if tag in ("ul", "ol"):
        _walk_children(node, style, out, images)
        return

    if tag == "a":
        _walk_children(node, style, out, images)
        return

    if tag in ("table", "tbody", "thead", "tr", "td", "th"):
        _walk_children(node, style, out, images)
        return

    _walk_children(node, style, out, images)


def _walk_children(node: Tag, style: StyleAccum, out: list, images: ImageRegistry) -> None:
    for child in node.children:
        _walk_node(child, style, out, images)


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
            item["value"] = re.sub(r"\n{3,}", "\n\n", item["value"])

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


def html_to_inline(html_str: str, images: ImageRegistry) -> list:
    """Convert an HTML string to a list of prova-pdf InlineContent dicts."""
    if not html_str:
        return []
    html_str = _strip_vml(html_str)
    soup = BeautifulSoup(html_str, "lxml")
    body = soup.find("body") or soup
    out: list = []
    _walk_children(body, StyleAccum(), out, images)
    return _collapse_text(out)


# ── Builders ──────────────────────────────────────────────────────────────────

def build_print_config(exam: dict) -> dict:
    """Build a PrintConfig dict from a merged exam+printconfig database row."""
    col = exam.get("column_type") or 0
    fs_idx = exam.get("font_size") or 0
    ls_idx = exam.get("line_height") or 0
    ds_idx = exam.get("discursive_question_space_type") or 0

    cfg: dict = {
        "columns": 2 if col == 1 else 1,
        "fontSize": FONT_SIZE_MAP.get(fs_idx, 12),
        "fontFamily": "body",
        "lineSpacing": LINE_SPACING_MAP.get(ls_idx, "normal"),
        "margins": {
            "top": float(exam.get("margin_top") or 0.6),
            "bottom": float(exam.get("margin_bottom") or 0.6),
            "left": float(exam.get("margin_left") or 0.6),
            "right": float(exam.get("margin_right") or 0.6),
        },
        "discursiveLineHeight": float(exam.get("discursive_line_height") or 0.85),
        "discursiveSpaceType": DISCURSIVE_SPACE_MAP.get(ds_idx, "lines"),
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

    if exam.get("uppercase_letters"):
        cfg["letterCase"] = "upper"
    else:
        cfg["letterCase"] = "lower"

    return cfg


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
        return {"type": "file"}

    if kind == "sum":
        return {"type": "sum", "items": []}

    if kind == "cloze":
        return {"type": "cloze", "wordBank": []}

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

    stem = html_to_inline(q["enunciation"], images)

    answer = build_answer(q, alts)
    if answer.get("_alternatives_raw"):
        parsed_alts = []
        for raw in answer.pop("_alternatives_raw"):
            parsed_alts.append({
                "label": raw["label"],
                "content": html_to_inline(raw["_raw_text"], images),
            })
        answer["alternatives"] = parsed_alts
        answer.pop("_alternatives_raw", None)

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

    return question
