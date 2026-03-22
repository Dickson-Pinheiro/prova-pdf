#!/usr/bin/env bash
# Cross-platform PDF generation test.
#
# Generates PDFs from each fixture via Python (WASI), Node.js (browser),
# and Go (WASI), then compares SHA-256 hashes.
#
# Usage: ./tests/cross-platform/run.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FONT="$ROOT/fonts/DejaVuSans.ttf"
FIXTURES_DIR="$ROOT/tests/fixtures"
OUT_DIR="/tmp/prova-pdf-cross-platform"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$OUT_DIR"

# Colours
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

FAILURES=0
TOTAL=0

echo "================================================================="
echo "  Cross-Platform PDF Generation Test"
echo "================================================================="
echo "  Font:    $FONT"
echo "  Output:  $OUT_DIR"
echo ""

for FIXTURE in "$FIXTURES_DIR"/all_kinds.json "$FIXTURES_DIR"/simple_choice.json; do
  [ -f "$FIXTURE" ] || continue
  NAME="$(basename "$FIXTURE" .json)"
  echo "─── $NAME ───"
  TOTAL=$((TOTAL + 1))

  # 1. Python WASI
  echo -n "  Python WASI:     "
  PY_OUT="$OUT_DIR/${NAME}_python.pdf"
  if PYTHONPATH="$ROOT/packages/python" python3 -c "
import sys, json
from prova_pdf import generate_pdf
font = open('$FONT', 'rb').read()
spec = json.load(open('$FIXTURE'))
pdf = generate_pdf(spec, [{'family': 'body', 'variant': 0, 'data': font}])
sys.stdout.buffer.write(pdf)
" > "$PY_OUT" 2>/tmp/prova-pdf-py-err; then
    PY_SIZE=$(wc -c < "$PY_OUT")
    PY_HASH=$(sha256sum "$PY_OUT" | cut -d' ' -f1)
    echo "${PY_SIZE} bytes  ${PY_HASH:0:16}…"
  else
    echo -e "${RED}FAIL$(cat /tmp/prova-pdf-py-err)${NC}"
    PY_HASH=""
    FAILURES=$((FAILURES + 1))
  fi

  # 2. Node.js browser
  echo -n "  Node.js browser: "
  NODE_OUT="$OUT_DIR/${NAME}_node.pdf"
  if node "$SCRIPT_DIR/node_generate.mjs" "$FIXTURE" "$FONT" > "$NODE_OUT" 2>/tmp/prova-pdf-node-err; then
    NODE_SIZE=$(wc -c < "$NODE_OUT")
    NODE_HASH=$(sha256sum "$NODE_OUT" | cut -d' ' -f1)
    echo "${NODE_SIZE} bytes  ${NODE_HASH:0:16}…"
  else
    echo -e "${RED}FAIL $(cat /tmp/prova-pdf-node-err)${NC}"
    NODE_HASH=""
    FAILURES=$((FAILURES + 1))
  fi

  # 3. Go WASI
  echo -n "  Go WASI:         "
  GO_OUT="$OUT_DIR/${NAME}_go.pdf"
  if (cd "$ROOT/packages/go" && /usr/local/go/bin/go run ./cmd/generate "$FIXTURE" "$FONT") > "$GO_OUT" 2>/tmp/prova-pdf-go-err; then
    GO_SIZE=$(wc -c < "$GO_OUT")
    GO_HASH=$(sha256sum "$GO_OUT" | cut -d' ' -f1)
    echo "${GO_SIZE} bytes  ${GO_HASH:0:16}…"
  else
    echo -e "${RED}FAIL $(cat /tmp/prova-pdf-go-err)${NC}"
    GO_HASH=""
    FAILURES=$((FAILURES + 1))
  fi

  # ── Comparisons ──
  # Python WASI vs Go WASI (same .wasm → must match)
  if [ -n "$PY_HASH" ] && [ -n "$GO_HASH" ]; then
    if [ "$PY_HASH" = "$GO_HASH" ]; then
      echo -e "  ${GREEN}✓ Python WASI == Go WASI (byte-identical)${NC}"
    else
      echo -e "  ${RED}✗ Python WASI != Go WASI (UNEXPECTED)${NC}"
      FAILURES=$((FAILURES + 1))
    fi
  fi

  # WASI vs Browser (different targets — may differ)
  if [ -n "$PY_HASH" ] && [ -n "$NODE_HASH" ]; then
    if [ "$PY_HASH" = "$NODE_HASH" ]; then
      echo -e "  ${GREEN}✓ WASI == Browser (byte-identical)${NC}"
    else
      echo -e "  ${YELLOW}△ WASI != Browser (different WASM targets)${NC}"
    fi
  fi

  echo ""
done

echo "================================================================="
if [ $FAILURES -eq 0 ]; then
  echo -e "  ${GREEN}ALL $TOTAL FIXTURE(S) PASSED${NC}"
else
  echo -e "  ${RED}$FAILURES FAILURE(S)${NC}"
fi
echo "  PDFs saved to: $OUT_DIR"
echo "================================================================="

exit $FAILURES
