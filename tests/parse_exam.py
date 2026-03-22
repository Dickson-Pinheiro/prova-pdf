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

import json
import sys
from collections import defaultdict
from pathlib import Path

import psycopg2
import psycopg2.extras

from exam_formatter import (
    BASE_TEXT_POSITION_MAP,
    ImageRegistry,
    build_print_config,
    build_question,
)

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
IMAGES_DIR = BASE_DIR / "fixtures" / "images" / "portugues_poema"

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
    """Fetch all exam data from the database. Returns plain dicts/lists."""
    cur = conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor)

    cur.execute(SQL_EXAM, (exam_id,))
    exam = cur.fetchone()
    if not exam:
        raise ValueError(f"Exam {exam_id} not found")
    exam = dict(exam)

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
    return exam, sections, questions, alts_by_qid, bts_by_qid


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
    images.base_dir = BASE_DIR.parent

    # Group questions by exam_teacher_subject_id
    qs_by_ets: dict = defaultdict(list)
    for q in questions:
        qs_by_ets[str(q["exam_teacher_subject_id"])].append(q)

    bt_position = BASE_TEXT_POSITION_MAP.get(exam["base_text_location"], "beforeQuestion")
    print(f"  base_text_location={exam['base_text_location']} → position={bt_position!r}")

    # Build sections
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
        if area_title:
            sec_json["title"] = area_title
        if subject_title:
            sec_json["_subject"] = subject_title
        sections_json.append(sec_json)

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

    loaded = json.loads(OUTPUT_PATH.read_text())
    total_q = sum(len(s["questions"]) for s in loaded["sections"])
    print(f"Validation: {len(loaded['sections'])} sections, {total_q} questions, "
          f"{len(loaded['_images'])} images — JSON OK")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--all":
        for name, eid in EXAM_PRESETS.items():
            globals()["EXAM_ID"] = eid
            globals()["OUTPUT_PATH"] = BASE_DIR / "fixtures" / f"{name}.json"
            globals()["IMAGES_DIR"] = BASE_DIR / "fixtures" / "images" / name
            print(f"\n{'='*60}\nGenerating {name} (exam {eid})\n{'='*60}", file=sys.stderr)
            main()
    elif len(sys.argv) > 1:
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
