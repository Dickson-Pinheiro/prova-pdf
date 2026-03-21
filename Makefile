.PHONY: build build-browser build-wasi build-all test clean size

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
