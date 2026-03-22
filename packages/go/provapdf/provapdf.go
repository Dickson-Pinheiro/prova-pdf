// Package provapdf generates exam PDFs via a WebAssembly module compiled from
// the prova-pdf Rust core. It uses wazero (pure-Go WebAssembly runtime) so
// there are no CGo dependencies.
//
// The embedded WASM file must be placed at packages/go/provapdf/prova_pdf.wasm
// before building. The project Makefile builds it to wasm/prova_pdf.wasm, so
// copy (or symlink) it:
//
//	cp wasm/prova_pdf.wasm packages/go/provapdf/prova_pdf.wasm
package provapdf

import (
	"context"
	"embed"
	"encoding/json"
	"fmt"
	"sync"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

//go:embed prova_pdf.wasm
var wasmFS embed.FS

// FontInput describes a font to register before PDF generation.
type FontInput struct {
	Family  string // role name, e.g. "body", "heading"
	Variant uint8  // 0=regular, 1=bold, 2=italic, 3=bold-italic
	Data    []byte // raw TTF or OTF bytes
}

// ImageInput describes an image to register before PDF generation.
type ImageInput struct {
	Key  string // key referenced by InlineContent or header.logoKey
	Data []byte // raw image bytes (PNG, JPEG, etc.)
}

// FontRules overrides font-role to family-name mappings.
type FontRules struct {
	Body     string `json:"body,omitempty"`
	Heading  string `json:"heading,omitempty"`
	Question string `json:"question,omitempty"`
	Math     string `json:"math,omitempty"`
}

// Option configures GeneratePDF behaviour.
type Option func(*options)

type options struct {
	images    []ImageInput
	fontRules *FontRules
}

// WithImages registers images before generation.
func WithImages(images []ImageInput) Option {
	return func(o *options) { o.images = images }
}

// WithFontRules overrides font-role mappings.
func WithFontRules(rules *FontRules) Option {
	return func(o *options) { o.fontRules = rules }
}

// ---------- singleton compiled module ----------

type compiledRuntime struct {
	ctx context.Context
	rt  wazero.Runtime
	mod wazero.CompiledModule
}

var (
	globalRT *compiledRuntime
	initOnce sync.Once
	initErr  error
)

func getRuntime() (*compiledRuntime, error) {
	initOnce.Do(func() {
		ctx := context.Background()
		rt := wazero.NewRuntime(ctx)

		wasi_snapshot_preview1.MustInstantiate(ctx, rt)

		wasmBytes, err := wasmFS.ReadFile("prova_pdf.wasm")
		if err != nil {
			initErr = fmt.Errorf("read embedded wasm: %w", err)
			return
		}

		compiled, err := rt.CompileModule(ctx, wasmBytes)
		if err != nil {
			initErr = fmt.Errorf("compile wasm: %w", err)
			return
		}

		globalRT = &compiledRuntime{ctx: ctx, rt: rt, mod: compiled}
	})
	return globalRT, initErr
}

// ---------- public API ----------

// GeneratePDF generates a PDF from an exam specification.
//
// spec is any JSON-serialisable value (typically map[string]any or a typed
// struct matching the prova-pdf ExamSpec schema).
//
// fonts must include at least one entry with Family="body" and Variant=0
// (regular).
func GeneratePDF(spec any, fonts []FontInput, opts ...Option) ([]byte, error) {
	r, err := getRuntime()
	if err != nil {
		return nil, err
	}

	cfg := &options{}
	for _, o := range opts {
		o(cfg)
	}

	ctx := r.ctx

	// Instantiate a fresh module per call for thread-safety (WASM is
	// single-threaded). WithName("") lets wazero auto-generate a unique name
	// so multiple instances can coexist.
	inst, err := r.rt.InstantiateModule(ctx, r.mod, wazero.NewModuleConfig().WithName(""))
	if err != nil {
		return nil, fmt.Errorf("instantiate module: %w", err)
	}
	defer inst.Close(ctx)

	// Exported functions
	fnAlloc := inst.ExportedFunction("prova_pdf_alloc")
	fnFree := inst.ExportedFunction("prova_pdf_free")
	fnAddFont := inst.ExportedFunction("prova_pdf_add_font")
	fnSetFontRules := inst.ExportedFunction("prova_pdf_set_font_rules")
	fnAddImage := inst.ExportedFunction("prova_pdf_add_image")
	fnClearAll := inst.ExportedFunction("prova_pdf_clear_all")
	fnGenerate := inst.ExportedFunction("prova_pdf_generate")
	fnOutputPtr := inst.ExportedFunction("prova_pdf_output_ptr")
	fnOutputLen := inst.ExportedFunction("prova_pdf_output_len")
	fnLastErrLen := inst.ExportedFunction("prova_pdf_last_error_len")
	fnLastErrMsg := inst.ExportedFunction("prova_pdf_last_error_message")

	mem := inst.Memory()

	// writeBytes allocates WASM memory, copies data into it, and returns the
	// pointer and length. The caller must free the allocation when done.
	writeBytes := func(data []byte) (ptr, length uint32, err error) {
		n := uint32(len(data))
		results, callErr := fnAlloc.Call(ctx, uint64(n))
		if callErr != nil {
			return 0, 0, callErr
		}
		p := uint32(results[0])
		if !mem.Write(p, data) {
			return 0, 0, fmt.Errorf("memory write failed at ptr=%d len=%d", p, n)
		}
		return p, n, nil
	}

	// readLastError extracts the error string stored in the WASM module.
	readLastError := func() string {
		results, callErr := fnLastErrLen.Call(ctx)
		if callErr != nil {
			return "unknown error (lastErrLen call failed)"
		}
		eLen := uint32(results[0])
		if eLen == 0 {
			return "unknown error"
		}
		results, callErr = fnAlloc.Call(ctx, uint64(eLen))
		if callErr != nil {
			return "unknown error (alloc for error msg failed)"
		}
		bufPtr := uint32(results[0])
		_, _ = fnLastErrMsg.Call(ctx, uint64(bufPtr))
		data, ok := mem.Read(bufPtr, eLen)
		_, _ = fnFree.Call(ctx, uint64(bufPtr), uint64(eLen))
		if !ok {
			return "unknown error (read error msg failed)"
		}
		return string(data)
	}

	// 1. Clear any leftover state (fresh instance, but be safe).
	_, _ = fnClearAll.Call(ctx)

	// 2. Register fonts.
	for _, f := range fonts {
		famBytes := []byte(f.Family)
		famPtr, famLen, err := writeBytes(famBytes)
		if err != nil {
			return nil, fmt.Errorf("alloc font family: %w", err)
		}
		dataPtr, dataLen, err := writeBytes(f.Data)
		if err != nil {
			_, _ = fnFree.Call(ctx, uint64(famPtr), uint64(famLen))
			return nil, fmt.Errorf("alloc font data: %w", err)
		}

		results, callErr := fnAddFont.Call(ctx,
			uint64(famPtr), uint64(famLen),
			uint64(f.Variant),
			uint64(dataPtr), uint64(dataLen),
		)
		_, _ = fnFree.Call(ctx, uint64(famPtr), uint64(famLen))
		_, _ = fnFree.Call(ctx, uint64(dataPtr), uint64(dataLen))
		if callErr != nil {
			return nil, fmt.Errorf("add_font call: %w", callErr)
		}
		if int32(results[0]) < 0 {
			return nil, fmt.Errorf("add_font failed: %s", readLastError())
		}
	}

	// 3. Register images.
	for _, img := range cfg.images {
		keyBytes := []byte(img.Key)
		keyPtr, keyLen, err := writeBytes(keyBytes)
		if err != nil {
			return nil, fmt.Errorf("alloc image key: %w", err)
		}
		dataPtr, dataLen, err := writeBytes(img.Data)
		if err != nil {
			_, _ = fnFree.Call(ctx, uint64(keyPtr), uint64(keyLen))
			return nil, fmt.Errorf("alloc image data: %w", err)
		}

		results, callErr := fnAddImage.Call(ctx,
			uint64(keyPtr), uint64(keyLen),
			uint64(dataPtr), uint64(dataLen),
		)
		_, _ = fnFree.Call(ctx, uint64(keyPtr), uint64(keyLen))
		_, _ = fnFree.Call(ctx, uint64(dataPtr), uint64(dataLen))
		if callErr != nil {
			return nil, fmt.Errorf("add_image call: %w", callErr)
		}
		if int32(results[0]) < 0 {
			return nil, fmt.Errorf("add_image failed: %s", readLastError())
		}
	}

	// 4. Set font rules (optional).
	if cfg.fontRules != nil {
		rulesJSON, jsonErr := json.Marshal(cfg.fontRules)
		if jsonErr != nil {
			return nil, fmt.Errorf("marshal font rules: %w", jsonErr)
		}
		rPtr, rLen, err := writeBytes(rulesJSON)
		if err != nil {
			return nil, fmt.Errorf("alloc font rules: %w", err)
		}
		results, callErr := fnSetFontRules.Call(ctx, uint64(rPtr), uint64(rLen))
		_, _ = fnFree.Call(ctx, uint64(rPtr), uint64(rLen))
		if callErr != nil {
			return nil, fmt.Errorf("set_font_rules call: %w", callErr)
		}
		if int32(results[0]) < 0 {
			return nil, fmt.Errorf("set_font_rules failed: %s", readLastError())
		}
	}

	// 5. Serialise spec to JSON.
	specJSON, jsonErr := json.Marshal(spec)
	if jsonErr != nil {
		return nil, fmt.Errorf("marshal spec: %w", jsonErr)
	}

	specPtr, specLen, err := writeBytes(specJSON)
	if err != nil {
		return nil, fmt.Errorf("alloc spec: %w", err)
	}

	// 6. Generate PDF. Pass out_buf=0, out_cap=0 so the module writes to its
	// internal staging buffer.
	results, callErr := fnGenerate.Call(ctx, uint64(specPtr), uint64(specLen), 0, 0)
	_, _ = fnFree.Call(ctx, uint64(specPtr), uint64(specLen))
	if callErr != nil {
		return nil, fmt.Errorf("generate call: %w", callErr)
	}

	rc := int32(results[0])
	if rc < 0 {
		return nil, fmt.Errorf("generate failed: %s", readLastError())
	}

	// 7. Read the PDF bytes from the staging buffer.
	ptrResults, _ := fnOutputPtr.Call(ctx)
	lenResults, _ := fnOutputLen.Call(ctx)
	outPtr := uint32(ptrResults[0])
	outLen := uint32(lenResults[0])

	pdfBytes, ok := mem.Read(outPtr, outLen)
	if !ok {
		return nil, fmt.Errorf("failed to read %d bytes from WASM memory at ptr=%d", outLen, outPtr)
	}

	// Return a copy — the WASM linear memory is freed when inst.Close runs.
	result := make([]byte, len(pdfBytes))
	copy(result, pdfBytes)
	return result, nil
}
