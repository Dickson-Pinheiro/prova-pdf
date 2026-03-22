"""
Parse a real exam from lize_master_db and produce a valid prova-pdf ExamSpec JSON.

Dependencies:
    pip install psycopg2-binary beautifulsoup4 requests lxml

Output:
    tests/fixtures/p4_lingua_portuguesa.json
    tests/fixtures/images/img_N.{png,jpg}
"""

import json
import re
import sys
from collections import defaultdict
from copy import copy
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional
from urllib.parse import urlparse

import psycopg2
import psycopg2.extras
import requests
from bs4 import BeautifulSoup, Comment, NavigableString, Tag
from PIL import Image
import io

# ── Constants ─────────────────────────────────────────────────────────────────

DB_CONFIG = dict(
    host="localhost",
    port=8888,
    dbname="lize_master_db",
    user="postgres",
    password="postgres",
)

EXAM_PRESETS = {
    "portugues_poema":     "2caa96b0-a28f-4820-838c-0240bc16d328",
    "pga_2em_2trimestre":  "ae5cd60b-d838-447e-961c-ddb9d8f47dc0",
    "exatas":              "ae5cd60b-d838-447e-961c-ddb9d8f47dc0",
    "matematica_vml":      "ba6442ec-0d04-4e3e-aaaa-fa99d720771a",
    "p4_lingua_portuguesa":"489dafa5-580d-47fe-8d41-1f6695495338",
}

# Default (overridden by CLI args)
EXAM_ID = EXAM_PRESETS["portugues_poema"]

BASE_DIR = Path(__file__).parent
OUTPUT_PATH = BASE_DIR / "fixtures" / "portugues_poema.json"
# Each exam stores images in its own subdirectory to avoid name collisions.
IMAGES_DIR = BASE_DIR / "fixtures" / "images" / "portugues_poema"

FONT_SIZE_MAP = {0: 12, 1: 14, 2: 18, 3: 22, 4: 32, 5: 8, 6: 10, 7: 11}
LINE_SPACING_MAP = {0: "normal", 1: "oneAndHalf", 2: "twoAndHalf", 3: "threeAndHalf"}
DISCURSIVE_SPACE_MAP = {0: "lines", 1: "blank", 2: "lines"}  # 0=per_question→default lines
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

    def copy(self) -> "StyleAccum":
        return copy(self)

    def to_dict(self) -> Optional[dict]:
        d = {}
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
    def __init__(self, images_dir: Path):
        self.images_dir = images_dir
        self.images_dir.mkdir(parents=True, exist_ok=True)
        self._url_to_key: dict[str, str] = {}
        self._key_to_dims: dict[str, tuple[int, int]] = {}
        self.key_to_path: dict[str, str] = {}
        self._counter = 0

    def register(self, url: str, width_px: int, height_px: int) -> dict:
        if url.startswith("data:"):
            url = self._save_data_uri(url)
            if url is None:
                return {"type": "text", "value": ""}
        if url in self._url_to_key:
            key = self._url_to_key[url]
            # Use stored dimensions (may have been filled from actual file).
            width_px, height_px = self._key_to_dims.get(key, (width_px, height_px))
        else:
            self._counter += 1
            key = f"img_{self._counter}"
            ext = self._guess_ext(url)
            filepath = self.images_dir / f"{key}{ext}"
            self._download(url, filepath)
            # If HTML didn't specify dimensions, read from the downloaded file.
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
            self.key_to_path[key] = str(filepath.relative_to(BASE_DIR.parent))

        node: dict = {"type": "image", "key": key}
        if width_px:
            node["widthCm"] = round(width_px / 96 * 2.54, 2)
        if height_px:
            node["heightCm"] = round(height_px / 96 * 2.54, 2)
        return node

    @staticmethod
    def _read_dims(filepath: Path):
        """Return (width_px, height_px) of a downloaded image, or None on error."""
        try:
            img = Image.open(filepath)
            return img.size
        except Exception:
            return None

    def _save_data_uri(self, data_uri: str) -> Optional[str]:
        """Decode a data:image/...;base64,... URI, save to disk, return a file:// path."""
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
            self.key_to_path[key] = str(filepath.relative_to(BASE_DIR.parent))
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
        # WebP and unknown formats → will be converted to PNG on download
        return ".png"

    @staticmethod
    def _download(url: str, dest: Path) -> None:
        try:
            r = requests.get(url, timeout=20)
            r.raise_for_status()
            content = r.content
            # Convert WebP (unsupported by prova-pdf WASM) to PNG
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
        return round(num * 0.75, 1)   # 1px = 0.75pt
    if unit in ("em", "rem"):
        return round(num * 12, 1)     # assume base 12pt
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
        return  # skip HTML comment content (includes VML conditional fragments)

    if isinstance(node, NavigableString):
        raw = str(node).replace("\xa0", " ")
        if raw:
            _emit_text_or_math(raw, style, out)
        return

    if not isinstance(node, Tag):
        return

    tag = node.name.lower() if node.name else ""

    # Skip TinyMCE artifacts
    if tag == "span" and "mce-nbsp-wrap" in (node.get("class") or []):
        return

    # MathML: extract LaTeX from <annotation encoding="application/x-tex"> if present,
    # otherwise fall back to the text content of the MathML element.
    if tag == "math":
        display = node.get("display") == "block"
        annotation = node.find("annotation", {"encoding": "application/x-tex"})
        if annotation:
            latex = annotation.get_text().strip()
            if latex:
                out.append({"type": "math", "latex": latex, "display": display})
                return
        # Fallback: plain text of all MathML child nodes
        text = node.get_text().strip()
        if text:
            out.append({"type": "text", "value": text})
        return

    # Line break
    if tag == "br":
        out.append({"type": "text", "value": "\n"})
        return

    # Images
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

    # Bold
    if tag in ("b", "strong"):
        ns = style.copy()
        ns.bold = True
        _walk_children(node, ns, out, images)
        return

    # Italic
    if tag in ("i", "em"):
        ns = style.copy()
        ns.italic = True
        _walk_children(node, ns, out, images)
        return

    # Underline
    if tag == "u":
        ns = style.copy()
        ns.underline = True
        _walk_children(node, ns, out, images)
        return

    # Subscript
    if tag == "sub":
        inner: list = []
        _walk_children(node, style.copy(), inner, images)
        inner = _collapse_text(inner)
        if inner:
            # MathType uses <sub><img/></sub> to position formula images inline.
            # These are not semantic subscripts — emit the images directly.
            if all(i.get("type") == "image" for i in inner):
                out.extend(inner)
            else:
                out.append({"type": "sub", "content": inner})
        return

    # Superscript
    if tag == "sup":
        inner = []
        _walk_children(node, style.copy(), inner, images)
        inner = _collapse_text(inner)
        if inner:
            # Same MathType pattern: <sup><img/></sup> → emit image directly.
            if all(i.get("type") == "image" for i in inner):
                out.extend(inner)
            else:
                out.append({"type": "sup", "content": inner})
        return

    # Span — may carry style
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

    # Block-level elements: inject paragraph separators
    if tag in BLOCK_TAGS:
        ns = style.copy()
        css = _parse_css_style(node.get("style", ""))
        # Detect poem verses (significant margin-left) and centered/right text
        margin_left = css.get("margin_left_pt", 0) or 0
        text_align = css.get("text_align", "")
        html_align = (node.get("align") or "").lower()
        if text_align == "center" or html_align == "center" or margin_left > 30:
            ns.text_align = "center"
        elif text_align == "right" or html_align == "right":
            ns.text_align = "right"
        # Add separator before block if out is non-empty and doesn't end with one
        if out and not _ends_with_separator(out):
            out.append({"type": "text", "value": "\n\n"})
        _walk_children(node, ns, out, images)
        # Add separator after block
        if out and not _ends_with_separator(out):
            out.append({"type": "text", "value": "\n\n"})
        return

    # Lists
    if tag in ("ul", "ol"):
        _walk_children(node, style, out, images)
        return

    # Anchors (ignore href)
    if tag == "a":
        _walk_children(node, style, out, images)
        return

    # Tables: flatten to inline text
    if tag in ("table", "tbody", "thead", "tr", "td", "th"):
        _walk_children(node, style, out, images)
        return

    # Passthrough: font, center, mark, s, del, ins, code, pre, etc.
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

    # Strip leading/trailing blank text nodes
    while result and result[0].get("type") == "text" and not result[0]["value"].strip():
        result.pop(0)
    while result and result[-1].get("type") == "text" and not result[-1]["value"].strip():
        result.pop()

    # Normalize: collapse 3+ consecutive newlines to \n\n (paragraph separator).
    # Two consecutive newlines (\n\n) come from block-level boundaries (<p> → <p>)
    # and should be preserved as paragraph breaks in the layout engine.
    # A single \n comes from an explicit <br> tag within a paragraph.
    for item in result:
        if item.get("type") == "text":
            item["value"] = re.sub(r"\n{3,}", "\n\n", item["value"])

    # Trim leading/trailing newlines from edge nodes
    if result and result[-1].get("type") == "text":
        result[-1]["value"] = result[-1]["value"].rstrip("\n")
        if not result[-1]["value"]:
            result.pop()

    if result and result[0].get("type") == "text":
        result[0]["value"] = result[0]["value"].lstrip("\n")
        if not result[0]["value"]:
            result.pop(0)

    # Trim only spaces/tabs (not newlines) from text nodes immediately adjacent to
    # block-level images (large figures).  Inline formula images (heightCm <= 1.5)
    # and math/sub/sup nodes need surrounding whitespace preserved.
    # Paragraph separators (\n\n) must be preserved so the layout engine can put
    # block images on their own visual lines.
    def _is_block_image(node: dict) -> bool:
        return node.get("type") == "image" and node.get("heightCm", 0) > 1.5

    for i, item in enumerate(result):
        if item.get("type") == "text":
            if i > 0 and _is_block_image(result[i - 1]):
                item["value"] = re.sub(r"^[ \t]+", "", item["value"])
            if i < len(result) - 1 and _is_block_image(result[i + 1]):
                item["value"] = re.sub(r"[ \t]+$", "", item["value"])

    # Remove empty text nodes after trimming
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
    if not html_str:
        return []
    html_str = _strip_vml(html_str)
    soup = BeautifulSoup(html_str, "lxml")
    # lxml wraps in <html><body>; walk body children
    body = soup.find("body") or soup
    out: list = []
    _walk_children(body, StyleAccum(), out, images)
    return _collapse_text(out)


# ── SQL queries ───────────────────────────────────────────────────────────────

SQL_EXAM = """
SELECT e.id, e.name, e.start_number, e.base_text_location,
       c.column_type, c.font_size, c.font_family, c.line_height,
       c.margin_top, c.margin_bottom, c.margin_left, c.margin_right,
       c.discursive_line_height, c.discursive_question_space_type,
       c.print_black_and_white_images, c.show_question_score,
       c.economy_mode, c.hide_numbering,
       c.hide_knowledge_areas_name, c.hide_questions_referencies,
       c.show_question_board, c.break_enunciation,
       c.break_alternatives, c.break_all_questions,
       c.force_choices_with_statement, c.remove_color_alternatives,
       c.uppercase_letters
FROM exams_exam e
LEFT JOIN clients_examprintconfig c ON e.exam_print_config_id = c.id
WHERE e.id = %s
"""

SQL_SECTIONS = """
SELECT ets.id, ets.order,
       s.name  AS subject_name,
       ka.name AS knowledge_area_name
FROM exams_examteachersubject ets
JOIN inspectors_teachersubject its ON ets.teacher_subject_id = its.id
JOIN subjects_subject s ON its.subject_id = s.id
LEFT JOIN subjects_knowledgearea ka ON s.knowledge_area_id = ka.id
WHERE ets.exam_id = %s
ORDER BY ets.order
"""

SQL_QUESTIONS = """
SELECT eq.exam_teacher_subject_id, eq.order as eq_order, eq.weight,
       q.id, q.category, q.is_essay, q.enunciation,
       q.quantity_lines, q.text_question_format, q.draft_rows_number,
       q.force_one_column, q.force_break_page, q.number_is_hidden
FROM exams_examquestion eq
JOIN questions_question q ON eq.question_id = q.id
WHERE eq.exam_id = %s
ORDER BY eq.exam_teacher_subject_id, eq.order
"""

SQL_ALTERNATIVES = """
SELECT question_id::text, index, text
FROM questions_questionoption
WHERE question_id = ANY(%s::uuid[])
ORDER BY question_id, index
"""

SQL_BASE_TEXTS = """
SELECT qbt.question_id::text, bt.id::text, bt.title, bt.text
FROM questions_question_base_texts qbt
JOIN questions_basetext bt ON qbt.basetext_id = bt.id
WHERE qbt.question_id = ANY(%s::uuid[])
ORDER BY qbt.question_id
"""


def fetch_exam_data(conn, exam_id: str) -> tuple:
    cur = conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor)

    # Q1: exam + printconfig
    cur.execute(SQL_EXAM, (exam_id,))
    exam = cur.fetchone()
    if not exam:
        raise ValueError(f"Exam {exam_id} not found")
    exam = dict(exam)

    # Q2: sections
    cur.execute(SQL_SECTIONS, (exam_id,))
    sections = [dict(r) for r in cur.fetchall()]

    # Q3: questions
    cur.execute(SQL_QUESTIONS, (exam_id,))
    questions = [dict(r) for r in cur.fetchall()]

    # Q4: alternatives (group by question_id)
    q_ids = [str(q["id"]) for q in questions]
    cur.execute(SQL_ALTERNATIVES, (q_ids,))
    alts_by_qid: dict = defaultdict(list)
    for row in cur.fetchall():
        alts_by_qid[str(row["question_id"])].append(dict(row))

    # Q5: base texts (group by question_id, deduplicate by bt.id)
    cur.execute(SQL_BASE_TEXTS, (q_ids,))
    bts_by_qid: dict = defaultdict(dict)  # qid → {bt_id: row}
    for row in cur.fetchall():
        qid = str(row["question_id"])
        btid = str(row["id"])
        if btid not in bts_by_qid[qid]:
            bts_by_qid[qid][btid] = dict(row)

    cur.close()
    return exam, sections, questions, alts_by_qid, bts_by_qid


# ── Builders ──────────────────────────────────────────────────────────────────

def build_print_config(exam: dict) -> dict:
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

    # Boolean flags — only include if True to keep JSON lean
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

    # force_choices_with_statement: boolean in DB → u8 in Rust (0 = off, 1 = on)
    if exam.get("force_choices_with_statement"):
        cfg["forceChoicesWithStatement"] = 1

    # uppercase_letters: boolean in DB → LetterCase enum in Rust
    if exam.get("uppercase_letters"):
        cfg["letterCase"] = "upper"
    else:
        cfg["letterCase"] = "lower"

    return cfg


def build_base_text(bt_row: dict, position: str, images: ImageRegistry) -> dict:
    content = html_to_inline(bt_row["text"], images)
    bt: dict = {
        "content": content,
        "position": position,
    }
    title = (bt_row.get("title") or "").strip()
    if title:
        bt["title"] = title
    return bt


def build_answer(q: dict, alts: list) -> dict:
    kind = "essay" if q["is_essay"] else CATEGORY_KIND_MAP.get(q["category"], "textual")

    if kind == "choice":
        alternatives = []
        for i, alt in enumerate(sorted(alts, key=lambda a: a["index"])):
            label = chr(ord("A") + i)
            alternatives.append({
                "label": label,
                "content": [],  # HTML parsed separately to avoid extra images arg here
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


def build_question(
    q: dict,
    alts: list,
    base_texts: list,
    bt_position: str,
    images: ImageRegistry,
    number: int,
) -> dict:
    kind = "essay" if q["is_essay"] else CATEGORY_KIND_MAP.get(q["category"], "textual")

    stem = html_to_inline(q["enunciation"], images)

    # Build answer with HTML-parsed alternative content
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

    # Base texts
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

    # Layout modifiers
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


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    print(f"Connecting to {DB_CONFIG['dbname']} on {DB_CONFIG['host']}:{DB_CONFIG['port']}…")
    conn = psycopg2.connect(**DB_CONFIG)
    conn.set_session(readonly=True)

    print(f"Fetching exam {EXAM_ID}…")
    exam, sections, questions, alts_by_qid, bts_by_qid = fetch_exam_data(conn, EXAM_ID)
    conn.close()

    print(f"Exam: {exam['name']}")
    print(f"  {len(sections)} sections, {len(questions)} questions")

    images = ImageRegistry(IMAGES_DIR)

    # Group questions by exam_teacher_subject_id, preserving order
    qs_by_ets: dict = defaultdict(list)
    for q in questions:
        qs_by_ets[str(q["exam_teacher_subject_id"])].append(q)

    bt_position = BASE_TEXT_POSITION_MAP.get(exam["base_text_location"], "beforeQuestion")
    print(f"  base_text_location={exam['base_text_location']} → position={bt_position!r}")

    # Build sections
    q_number = exam["start_number"]
    sections_json = []
    assigned_bt_ids: set = set()  # tracks base text IDs already assigned to a question
    for sec in sections:
        ets_id = str(sec["id"])
        sec_questions = qs_by_ets.get(ets_id, [])
        questions_json = []
        for q in sec_questions:
            qid = str(q["id"])
            alts = alts_by_qid.get(qid, [])
            # Each base text is rendered only at the first (lowest-numbered) question
            # that references it — matching lize HTML's getTheLowestNumber() logic.
            all_bts = list(bts_by_qid.get(qid, {}).values())
            bts = [bt for bt in all_bts if bt["id"] not in assigned_bt_ids]
            for bt in bts:
                assigned_bt_ids.add(bt["id"])
            q_json = build_question(q, alts, bts, bt_position, images, q_number)
            questions_json.append(q_json)
            q_number += 1
            print(
                f"  [{q_number-1:02d}] kind={q_json['kind']:8s}  alts={len(alts)}  bts={len(bts)}",
                file=sys.stderr,
            )

        sec_json: dict = {"questions": questions_json}
        # Store both title variants so consumers can choose which to display:
        #   "title"    → knowledge_area_name (e.g. "Ciências da Natureza e suas Tecnologias - Ensino Médio")
        #   "_subject" → subject_name        (e.g. "Biologia")
        # The "_subject" prefix is ignored by the Rust deserializer (like "_images").
        subject_title = (sec.get("subject_name") or "").strip()
        area_title    = (sec.get("knowledge_area_name") or subject_title).strip()
        if area_title:
            sec_json["title"] = area_title
        if subject_title:
            sec_json["_subject"] = subject_title
        sections_json.append(sec_json)

    # Build config (exam row carries merged printconfig columns)
    config = build_print_config(exam)

    output = {
        "_images": images.key_to_path,
        "metadata": {"title": exam["name"]},
        "config": config,
        "header": {
            "title": exam["name"],
            "studentFields": [
                {"label": "Nome"},
                {"label": "Turma", "widthCm": 5},
                {"label": "Data", "widthCm": 4},
                {"label": "Nota", "widthCm": 3},
            ],
        },
        "sections": sections_json,
    }

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(json.dumps(output, ensure_ascii=False, indent=2))
    print(f"\nSaved → {OUTPUT_PATH}")
    print(f"Images → {IMAGES_DIR} ({len(images.key_to_path)} files)")

    # Quick validation
    loaded = json.loads(OUTPUT_PATH.read_text())
    total_q = sum(len(s["questions"]) for s in loaded["sections"])
    print(f"Validation: {len(loaded['sections'])} sections, {total_q} questions, "
          f"{len(loaded['_images'])} images — JSON OK")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--all":
        # Regenerate all fixture presets
        for name, eid in EXAM_PRESETS.items():
            EXAM_ID_RUN = eid
            OUTPUT_PATH_RUN = BASE_DIR / "fixtures" / f"{name}.json"
            IMAGES_DIR_RUN = BASE_DIR / "fixtures" / "images" / name
            # Patch globals so main() picks them up
            globals()["EXAM_ID"] = EXAM_ID_RUN
            globals()["OUTPUT_PATH"] = OUTPUT_PATH_RUN
            globals()["IMAGES_DIR"] = IMAGES_DIR_RUN
            print(f"\n{'='*60}\nGenerating {name} (exam {eid})\n{'='*60}", file=sys.stderr)
            main()
    elif len(sys.argv) > 1:
        # Single preset by name: python parse_exam.py portugues_poema
        name = sys.argv[1]
        if name in EXAM_PRESETS:
            globals()["EXAM_ID"] = EXAM_PRESETS[name]
            globals()["OUTPUT_PATH"] = BASE_DIR / "fixtures" / f"{name}.json"
            globals()["IMAGES_DIR"] = BASE_DIR / "fixtures" / "images" / name
        else:
            print(f"Unknown preset '{name}'. Available: {', '.join(EXAM_PRESETS.keys())}", file=sys.stderr)
            sys.exit(1)
        main()
    else:
        main()
