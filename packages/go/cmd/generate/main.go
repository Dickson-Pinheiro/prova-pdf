// Command generate produces a PDF from a JSON ExamSpec fixture.
//
// Usage:
//
//	generate [flags] <fixture.json>
//
// Flags:
//
//	-font family:variant:path   Register a font (repeatable).
//	                            family  — role name, e.g. "body"
//	                            variant — 0=regular 1=bold 2=italic 3=bold-italic
//	                            path    — path to TTF/OTF file
//	-o path                     Write PDF to file instead of stdout.
//	-images-from-spec           Load images listed in the fixture's "_images" map.
//
// Examples:
//
//	# Single font, output to stdout
//	generate -font body:0:regular.ttf fixture.json > out.pdf
//
//	# Full IBM Plex Sans with images, write to file
//	generate \
//	  -font body:0:IBMPlexSans-Regular.ttf \
//	  -font body:1:IBMPlexSans-Bold.ttf \
//	  -font body:2:IBMPlexSans-Italic.ttf \
//	  -font body:3:IBMPlexSans-BoldItalic.ttf \
//	  -images-from-spec \
//	  -o out.pdf \
//	  fixture.json
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/Dickson-Pinheiro/prova-pdf/packages/go/provapdf"
)

// fontFlag supports repeated -font flags.
type fontFlag []provapdf.FontInput

func (f *fontFlag) String() string { return fmt.Sprint(*f) }

func (f *fontFlag) Set(val string) error {
	// Expected format: "family:variant:path"
	parts := strings.SplitN(val, ":", 3)
	if len(parts) != 3 {
		return fmt.Errorf("invalid font spec %q — want family:variant:path", val)
	}
	v, err := strconv.ParseUint(parts[1], 10, 8)
	if err != nil {
		return fmt.Errorf("invalid variant %q: %w", parts[1], err)
	}
	data, err := os.ReadFile(parts[2])
	if err != nil {
		return fmt.Errorf("read font %q: %w", parts[2], err)
	}
	*f = append(*f, provapdf.FontInput{
		Family:  parts[0],
		Variant: uint8(v),
		Data:    data,
	})
	return nil
}

func main() {
	var fonts fontFlag
	outPath := flag.String("o", "", "write PDF to `path` (default: stdout)")
	loadImages := flag.Bool("images-from-spec", false, "load images from the fixture's \"_images\" map")

	flag.Var(&fonts, "font", "font spec: family:variant:path (repeatable)")
	flag.Usage = func() {
		fmt.Fprintln(os.Stderr, "usage: generate [flags] <fixture.json>")
		flag.PrintDefaults()
	}
	flag.Parse()

	if flag.NArg() != 1 {
		flag.Usage()
		os.Exit(1)
	}
	if len(fonts) == 0 {
		fmt.Fprintln(os.Stderr, "error: at least one -font flag is required")
		flag.Usage()
		os.Exit(1)
	}

	specBytes, err := os.ReadFile(flag.Arg(0))
	if err != nil {
		fmt.Fprintf(os.Stderr, "read spec: %v\n", err)
		os.Exit(1)
	}

	var spec map[string]any
	if err := json.Unmarshal(specBytes, &spec); err != nil {
		fmt.Fprintf(os.Stderr, "parse spec: %v\n", err)
		os.Exit(1)
	}

	var opts []provapdf.Option

	// Load images referenced by the fixture's "_images" map.
	if *loadImages {
		images, imgErr := loadSpecImages(spec, flag.Arg(0))
		if imgErr != nil {
			fmt.Fprintf(os.Stderr, "load images: %v\n", imgErr)
			os.Exit(1)
		}
		if len(images) > 0 {
			opts = append(opts, provapdf.WithImages(images))
			fmt.Fprintf(os.Stderr, "loaded %d image(s)\n", len(images))
		}
	}

	pdf, err := provapdf.GeneratePDF(spec, []provapdf.FontInput(fonts), opts...)
	if err != nil {
		fmt.Fprintf(os.Stderr, "generate: %v\n", err)
		os.Exit(1)
	}

	if *outPath != "" {
		if err := os.WriteFile(*outPath, pdf, 0o644); err != nil {
			fmt.Fprintf(os.Stderr, "write output: %v\n", err)
			os.Exit(1)
		}
		fmt.Fprintf(os.Stderr, "wrote %d bytes → %s\n", len(pdf), *outPath)
	} else {
		os.Stdout.Write(pdf)
	}
}

// findRepoRoot walks up from dir until it finds a directory containing Cargo.toml
// (the repo root marker). Falls back to dir itself if not found.
func findRepoRoot(dir string) string {
	current := dir
	for {
		if _, err := os.Stat(filepath.Join(current, "Cargo.toml")); err == nil {
			return current
		}
		parent := filepath.Dir(current)
		if parent == current {
			return dir // reached filesystem root, give up
		}
		current = parent
	}
}

// loadSpecImages reads the "_images" map from the spec fixture and loads each
// image file. Paths in "_images" may be absolute or relative to the repo root.
func loadSpecImages(spec map[string]any, fixturePath string) ([]provapdf.ImageInput, error) {
	raw, ok := spec["_images"]
	if !ok {
		return nil, nil
	}
	imgMap, ok := raw.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("_images must be an object")
	}

	// _images paths are either absolute or relative to the repo root.
	// For relative paths, resolve from the fixture file's directory upward
	// until finding a directory containing go.mod (repo root marker).
	fixtureDir := filepath.Dir(fixturePath)
	fixtureAbs, _ := filepath.Abs(fixtureDir)
	repoRoot := findRepoRoot(fixtureAbs)

	var images []provapdf.ImageInput
	for key, pathAny := range imgMap {
		relPath, ok := pathAny.(string)
		if !ok {
			continue
		}
		var absPath string
		if filepath.IsAbs(relPath) {
			absPath = relPath
		} else {
			absPath = filepath.Join(repoRoot, relPath)
		}
		data, err := os.ReadFile(absPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "warn: skip image %q (%s): %v\n", key, relPath, err)
			continue
		}
		images = append(images, provapdf.ImageInput{Key: key, Data: data})
	}
	return images, nil
}
