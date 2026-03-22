.PHONY: build build-browser build-wasi build-all test clean size size-check bench

# ─── Build targets ────────────────────────────────────────────────────────────

## Browser package (wasm-bindgen, JS/TS target)
build-browser:
	cargo build --target wasm32-unknown-unknown \
	  --features browser,math,images --no-default-features --release
	wasm-bindgen --target web \
	  target/wasm32-unknown-unknown/release/prova_pdf.wasm --out-dir pkg/
	@if command -v wasm-opt >/dev/null 2>&1; then \
	  echo "Running wasm-opt..."; \
	  wasm-opt -Oz --strip-debug --strip-producers \
	    --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int \
	    pkg/prova_pdf_bg.wasm -o pkg/prova_pdf_bg.wasm; \
	else \
	  echo "wasm-opt not found, skipping"; \
	fi
	cp npm/package.json pkg/package.json
	cp npm/prova-pdf.d.ts pkg/prova-pdf.d.ts
	cp npm/index.js pkg/index.js
	@echo "Browser build complete → pkg/"

## WASI library (C-ABI, for Python/Go/CLI)
build-wasi:
	cargo build --target wasm32-wasip1 \
	  --features wasi-lib,math,images --no-default-features --release
	mkdir -p wasm
	cp target/wasm32-wasip1/release/prova_pdf.wasm wasm/prova_pdf.wasm
	@if command -v wasm-opt >/dev/null 2>&1; then \
	  echo "Running wasm-opt..."; \
	  wasm-opt -Oz --strip-debug --strip-producers \
	    --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int \
	    wasm/prova_pdf.wasm -o wasm/prova_pdf.wasm; \
	fi
	@echo "WASI build complete → wasm/prova_pdf.wasm"

## Both targets
build-all: build-browser build-wasi

build: build-all

# ─── Test targets ─────────────────────────────────────────────────────────────

## Rust unit tests (native)
test:
	cargo test

## All tests
test-all: test

# ─── Utility ──────────────────────────────────────────────────────────────────

clean:
	cargo clean
	rm -rf pkg/ wasm/

size:
	@[ -f pkg/prova_pdf_bg.wasm ] && { \
	  raw=$$(ls -lh pkg/prova_pdf_bg.wasm | awk '{print $$5}'); \
	  gz=$$(gzip -c pkg/prova_pdf_bg.wasm | wc -c | awk '{printf "%.0fKB", $$1/1024}'); \
	  echo "Browser WASM: $$raw raw / $$gz gzipped"; \
	} || true
	@[ -f wasm/prova_pdf.wasm ] && { \
	  raw=$$(ls -lh wasm/prova_pdf.wasm | awk '{print $$5}'); \
	  gz=$$(gzip -c wasm/prova_pdf.wasm | wc -c | awk '{printf "%.0fKB", $$1/1024}'); \
	  echo "WASI WASM:    $$raw raw / $$gz gzipped"; \
	} || true

## Validate WASM sizes against PROJECT.md thresholds.
## Builds 3 feature combinations and checks gzipped sizes.
## Exit non-zero if any threshold is exceeded.
size-check:
	@echo "=== Building browser (math+images) ==="
	@cargo build --target wasm32-unknown-unknown \
	  --features browser,math,images --no-default-features --release 2>/dev/null
	@wasm-bindgen --target web \
	  target/wasm32-unknown-unknown/release/prova_pdf.wasm --out-dir /tmp/prova-size-full 2>/dev/null
	@if command -v wasm-opt >/dev/null 2>&1; then \
	  wasm-opt -Oz --strip-debug --strip-producers \
	    --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int \
	    /tmp/prova-size-full/prova_pdf_bg.wasm -o /tmp/prova-size-full/prova_pdf_bg.wasm 2>/dev/null; \
	fi
	@gz_full=$$(gzip -c /tmp/prova-size-full/prova_pdf_bg.wasm | wc -c); \
	  gz_kb=$$((gz_full / 1024)); \
	  echo "  math+images: $${gz_kb}KB gzipped (threshold: 900KB)"; \
	  if [ $$gz_kb -gt 900 ]; then echo "  FAIL: exceeds 900KB threshold"; exit 1; fi
	@echo "=== Building browser (math only) ==="
	@cargo build --target wasm32-unknown-unknown \
	  --features browser,math --no-default-features --release 2>/dev/null
	@wasm-bindgen --target web \
	  target/wasm32-unknown-unknown/release/prova_pdf.wasm --out-dir /tmp/prova-size-math 2>/dev/null
	@if command -v wasm-opt >/dev/null 2>&1; then \
	  wasm-opt -Oz --strip-debug --strip-producers \
	    --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int \
	    /tmp/prova-size-math/prova_pdf_bg.wasm -o /tmp/prova-size-math/prova_pdf_bg.wasm 2>/dev/null; \
	fi
	@gz_math=$$(gzip -c /tmp/prova-size-math/prova_pdf_bg.wasm | wc -c); \
	  gz_kb=$$((gz_math / 1024)); \
	  echo "  math only:   $${gz_kb}KB gzipped (threshold: 750KB)"; \
	  if [ $$gz_kb -gt 750 ]; then echo "  FAIL: exceeds 750KB threshold"; exit 1; fi
	@echo "=== Building browser (minimal) ==="
	@cargo build --target wasm32-unknown-unknown \
	  --features browser --no-default-features --release 2>/dev/null
	@wasm-bindgen --target web \
	  target/wasm32-unknown-unknown/release/prova_pdf.wasm --out-dir /tmp/prova-size-min 2>/dev/null
	@if command -v wasm-opt >/dev/null 2>&1; then \
	  wasm-opt -Oz --strip-debug --strip-producers \
	    --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int \
	    /tmp/prova-size-min/prova_pdf_bg.wasm -o /tmp/prova-size-min/prova_pdf_bg.wasm 2>/dev/null; \
	fi
	@gz_min=$$(gzip -c /tmp/prova-size-min/prova_pdf_bg.wasm | wc -c); \
	  gz_kb=$$((gz_min / 1024)); \
	  echo "  minimal:     $${gz_kb}KB gzipped (threshold: 500KB)"; \
	  if [ $$gz_kb -gt 500 ]; then echo "  FAIL: exceeds 500KB threshold"; exit 1; fi
	@echo "=== All size checks passed ==="
	@rm -rf /tmp/prova-size-full /tmp/prova-size-math /tmp/prova-size-min

## Run Criterion benchmarks (native)
bench:
	cargo bench
