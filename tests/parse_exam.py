"""
Parse a real exam from lize_master_db and produce a valid prova-pdf ExamSpec JSON.

This script handles ONLY database access and orchestration.
All formatting logic lives in exam_formatter.py (reusable in lize Django).

Dependencies:
    pip install psycopg2-binary beautifulsoup4 requests lxml Pillow

Output:
    tests/fixtures/<preset_name>.json
    tests/fixtures/images/<preset_name>/img_N.{png,jpg}
"""

import argparse
import json
import os
import sys
from collections import defaultdict
from pathlib import Path

import psycopg2
import psycopg2.extras

from exam_formatter import (
    BASE_TEXT_POSITION_MAP,
    ImageRegistry,
    build_header,
    build_print_config,
    build_question,
    build_url_params,
)

# ── Constants ─────────────────────────────────────────────────────────────────

DB_CONFIG = dict(
    host=os.environ.get("LIZE_DB_HOST", "localhost"),
    port=int(os.environ.get("LIZE_DB_PORT", "8888")),
    dbname=os.environ.get("LIZE_DB_NAME", "lize_master_db"),
    user=os.environ.get("LIZE_DB_USER", "postgres"),
    password=os.environ.get("LIZE_DB_PASSWORD", "postgres"),
)

DJANGO_BASE_URL = os.environ.get("DJANGO_BASE_URL", "http://localhost:8000")

# DigitalOcean Spaces CDN domain (matches SPACES_AWS_S3_CUSTOM_DOMAIN in .env).
# Used to build media URLs when the file is not available via the local Django server.
_S3_CUSTOM_DOMAIN = os.environ.get(
    "SPACES_AWS_S3_CUSTOM_DOMAIN",
    "fiscallizeremote.nyc3.cdn.digitaloceanspaces.com/fiscallizeremote",
)
MEDIA_BASE_URL = os.environ.get(
    "MEDIA_BASE_URL",
    f"https://{_S3_CUSTOM_DOMAIN}/media",
)

EXAM_PRESETS = {
    "portugues_poema":     "2caa96b0-a28f-4820-838c-0240bc16d328",
    "pga_2em_2trimestre":  "ae5cd60b-d838-447e-961c-ddb9d8f47dc0",
    "exatas":              "ae5cd60b-d838-447e-961c-ddb9d8f47dc0",
    "matematica_vml":      "ba6442ec-0d04-4e3e-aaaa-fa99d720771a",
    "p4_lingua_portuguesa":"489dafa5-580d-47fe-8d41-1f6695495338",
}

BASE_DIR = Path(__file__).parent

# ── SQL queries ───────────────────────────────────────────────────────────────

SQL_EXAM = """
SELECT e.id, e.name, e.start_number, e.base_text_location,
       e.external_code, e.orientations,
       c.column_type, c.font_size, c.font_family, c.line_height,
       c.margin_top, c.margin_bottom, c.margin_left, c.margin_right,
       c.discursive_line_height, c.discursive_question_space_type,
       c.print_black_and_white_images, c.show_question_score,
       c.economy_mode, c.hide_numbering,
       c.hide_knowledge_areas_name, c.hide_questions_referencies,
       c.show_question_board, c.break_enunciation,
       c.break_alternatives, c.break_all_questions,
       c.force_choices_with_statement, c.remove_color_alternatives,
       c.uppercase_letters,
       c.header_format, c.kind, c.hyphenate, c.show_footer,
       c.add_page_number, c.print_subjects_name,
       c.hide_alternatives_indicator,
       c.header_id AS exam_header_id,
       c2.name AS client_name, c2.id AS client_pk, c2.logo AS client_logo
FROM exams_exam e
LEFT JOIN clients_examprintconfig c ON e.exam_print_config_id = c.id
-- Auto-generated M2M table linking Exam → SchoolCoordination (no explicit through model)
LEFT JOIN exams_exam_coordinations eec ON eec.exam_id = e.id
LEFT JOIN clients_schoolcoordination sc ON sc.id = eec.schoolcoordination_id
LEFT JOIN clients_unity u ON u.id = sc.unity_id
LEFT JOIN clients_client c2 ON c2.id = u.client_id
-- Note: 'language' and 'hide_discipline_name' do not exist in ExamPrintConfig model
WHERE e.id = %s
LIMIT 1
"""

SQL_SECTIONS = """
SELECT ets.id, ets.order,
       s.name  AS subject_name,
       ka.name AS knowledge_area_name,
       g.name  AS grade_name
FROM exams_examteachersubject ets
JOIN inspectors_teachersubject its ON ets.teacher_subject_id = its.id
JOIN subjects_subject s ON its.subject_id = s.id
LEFT JOIN subjects_knowledgearea ka ON s.knowledge_area_id = ka.id
LEFT JOIN classes_grade g ON g.id = ets.grade_id
WHERE ets.exam_id = %s
ORDER BY ets.order
"""

SQL_QUESTIONS = """
SELECT eq.exam_teacher_subject_id, eq.order as eq_order, eq.weight,
       q.id, q.category, q.is_essay, q.enunciation,
       q.quantity_lines, q.text_question_format, q.draft_rows_number,
       q.force_one_column, q.force_break_page, q.number_is_hidden,
       q.board, q.level, q.theme, q.cloze_content,
       q.break_enunciation, q.break_alternatives,
       q.force_choices_with_statement, q.print_only_enunciation,
       q.commented_awnser, q.feedback,
       q.support_content_question, q.support_content_position,
       q.subject_id
FROM exams_examquestion eq
JOIN questions_question q ON eq.question_id = q.id
WHERE eq.exam_id = %s
ORDER BY eq.exam_teacher_subject_id, eq.order
"""

SQL_ALTERNATIVES = """
SELECT question_id::text, index, text, is_correct
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
-- Note: BaseText has no subject_id field in the Django model, so it is not selected here.
"""

SQL_EXAM_HEADER = """
SELECT content FROM exams_examheader WHERE id = %s
"""


def fetch_exam_data(conn, exam_id: str) -> tuple:
    """Fetch all exam data from the database. Returns plain dicts/lists.

    Returns:
        (exam, sections, questions, alts_by_qid, bts_by_qid, exam_header)
        exam_header is a dict with 'content' key, or None if not set.
    """
    cur = conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor)

    cur.execute(SQL_EXAM, (exam_id,))
    exam = cur.fetchone()
    if not exam:
        raise ValueError(f"Exam {exam_id} not found")
    exam = dict(exam)

    # Fetch ExamHeader content if a header is configured on the ExamPrintConfig
    exam_header = None
    if exam.get("exam_header_id") is not None:
        cur.execute(SQL_EXAM_HEADER, (exam["exam_header_id"],))
        row = cur.fetchone()
        if row:
            exam_header = dict(row)

    cur.execute(SQL_SECTIONS, (exam_id,))
    sections = [dict(r) for r in cur.fetchall()]

    cur.execute(SQL_QUESTIONS, (exam_id,))
    questions = [dict(r) for r in cur.fetchall()]

    q_ids = [str(q["id"]) for q in questions]
    cur.execute(SQL_ALTERNATIVES, (q_ids,))
    alts_by_qid: dict = defaultdict(list)
    for row in cur.fetchall():
        alts_by_qid[str(row["question_id"])].append(dict(row))

    cur.execute(SQL_BASE_TEXTS, (q_ids,))
    bts_by_qid: dict = defaultdict(dict)
    for row in cur.fetchall():
        qid = str(row["question_id"])
        btid = str(row["id"])
        if btid not in bts_by_qid[qid]:
            bts_by_qid[qid][btid] = dict(row)

    cur.close()
    return exam, sections, questions, alts_by_qid, bts_by_qid, exam_header


# ── Public API ────────────────────────────────────────────────────────────────

def build_spec(
    exam_id: str,
    *,
    include_sections: bool = True,
    include_header: bool = True,
    header_only: bool = False,
    section_id: str = None,
    question_id: str = None,
    overrides: dict = None,
) -> dict:
    """
    Conecta ao banco, busca dados do exam_id e retorna ExamSpec dict completo.

    overrides: dict com chaves opcionais paper_size, all_black para build_print_config.
    """
    if overrides is None:
        overrides = {}

    print(f"Connecting to {DB_CONFIG['dbname']} on {DB_CONFIG['host']}:{DB_CONFIG['port']}…")
    conn = psycopg2.connect(**DB_CONFIG)
    conn.set_session(readonly=True)

    print(f"Fetching exam {exam_id}…")
    exam, sections, questions, alts_by_qid, bts_by_qid, exam_header = fetch_exam_data(conn, exam_id)
    conn.close()

    print(f"Exam: {exam['name']}")
    print(f"  {len(sections)} sections, {len(questions)} questions")

    # Derive images directory from exam_id so each exam has its own folder
    images_dir = BASE_DIR / "fixtures" / "images" / exam_id
    images = ImageRegistry(images_dir)
    images.base_dir = BASE_DIR.parent

    # Download client logo with a fixed key so header.logoKey resolves correctly.
    # client_logo in DB is a relative media path (e.g. "clients/logos/logo.jpg").
    # Try Django local first; fall back to DigitalOcean Spaces CDN.
    if exam.get("client_logo"):
        logo_rel = str(exam["client_logo"]).lstrip("/")
        if logo_rel.startswith(("http://", "https://")):
            logo_url = logo_rel
        else:
            logo_url = f"{DJANGO_BASE_URL.rstrip('/')}/media/{logo_rel}"
        ok = images.register_as(logo_url, "client_logo")
        if not ok:
            # Local server doesn't have the file — try S3 CDN
            s3_url = f"{MEDIA_BASE_URL.rstrip('/')}/{logo_rel}"
            print(f"  → Trying S3 CDN: {s3_url}", file=sys.stderr)
            ok = images.register_as(s3_url, "client_logo")
        if not ok:
            print("  WARN: client_logo download failed; logoKey will be omitted", file=sys.stderr)
            exam = dict(exam, client_logo=None)

    # Apply section filter
    if section_id is not None:
        sections = [s for s in sections if str(s["id"]) == str(section_id)]

    # Apply question filter (filter questions list; sections still iterated as usual)
    if question_id is not None:
        questions = [q for q in questions if str(q["id"]) == str(question_id)]

    # Build sections JSON
    if header_only or not include_sections:
        sections_json = []
    else:
        # Group questions by exam_teacher_subject_id
        qs_by_ets: dict = defaultdict(list)
        for q in questions:
            qs_by_ets[str(q["exam_teacher_subject_id"])].append(q)

        bt_position = BASE_TEXT_POSITION_MAP.get(exam["base_text_location"], "beforeQuestion")
        print(f"  base_text_location={exam['base_text_location']} → position={bt_position!r}")

        q_number = exam["start_number"]
        sections_json = []
        assigned_bt_ids: set = set()
        for sec in sections:
            ets_id = str(sec["id"])
            sec_questions = qs_by_ets.get(ets_id, [])
            questions_json = []
            for q in sec_questions:
                qid = str(q["id"])
                alts = alts_by_qid.get(qid, [])
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
            subject_title = (sec.get("subject_name") or "").strip()
            area_title    = (sec.get("knowledge_area_name") or subject_title).strip()

            hide_ka   = bool(exam.get("hide_knowledge_areas_name"))
            hide_disc = not bool(exam.get("print_subjects_name"))

            if area_title and not hide_ka:
                sec_json["title"] = area_title
            elif subject_title and not hide_disc:
                sec_json["title"] = subject_title

            if subject_title:
                sec_json["_subject"] = subject_title
            sections_json.append(sec_json)

    config = build_print_config(exam, overrides=overrides)
    header = build_header(exam, exam, exam_header) if include_header else {}

    return {
        "_images": images.key_to_path,
        "_url_params": build_url_params(exam),
        "metadata": {"title": exam["name"]},
        "config": config,
        "header": header,
        "sections": sections_json,
    }


# ── CLI helpers ───────────────────────────────────────────────────────────────

def parse_args():
    p = argparse.ArgumentParser(description="Gera ExamSpec JSON a partir do lize_master_db")
    p.add_argument("preset", nargs="?", help="Nome do preset (ver EXAM_PRESETS) ou UUID direto")
    p.add_argument("--all", action="store_true", help="Gerar todos os presets")
    p.add_argument("--out", metavar="PATH", help="Arquivo de saída (default: tests/fixtures/<preset>.json)")
    p.add_argument("--header-only", action="store_true", help="Emitir ExamSpec com sections=[]")
    p.add_argument("--section", metavar="ETS_ID", help="Incluir apenas a seção com este ExamTeacherSubject ID")
    p.add_argument("--question", metavar="QUESTION_ID", help="Incluir apenas a questão com este ID")
    p.add_argument("--paper-size", default=None, choices=["A4", "Ata"], help="Override de paper_size")
    p.add_argument("--all-black", action="store_true", help="Override all_black=True")
    return p.parse_args()


def _write_spec(spec: dict, out_path: Path, name: str) -> None:
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(spec, ensure_ascii=False, indent=2))
    total_q = sum(len(s.get("questions", [])) for s in spec.get("sections", []))
    print(f"[{name}] {len(spec.get('sections', []))} seções, {total_q} questões, "
          f"{len(spec.get('_images', {}))} imagens → {out_path}")


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    args = parse_args()
    overrides = {}
    if args.paper_size:
        overrides["paper_size"] = args.paper_size
    if args.all_black:
        overrides["all_black"] = True

    if args.all:
        for name, eid in EXAM_PRESETS.items():
            out = BASE_DIR / "fixtures" / f"{name}.json"
            spec = build_spec(eid, overrides=overrides)
            _write_spec(spec, out, name)
        return

    # Resolver exam_id
    preset_name = args.preset or "portugues_poema"
    if preset_name in EXAM_PRESETS:
        exam_id = EXAM_PRESETS[preset_name]
    else:
        exam_id = preset_name  # assume UUID direto

    out_path = Path(args.out) if args.out else BASE_DIR / "fixtures" / f"{preset_name}.json"

    spec = build_spec(
        exam_id,
        header_only=args.header_only,
        section_id=args.section,
        question_id=args.question,
        overrides=overrides,
    )
    _write_spec(spec, out_path, preset_name)


if __name__ == "__main__":
    main()
