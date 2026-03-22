// Command generate produces a PDF from a JSON fixture and writes it to stdout.
//
// Usage: go run ./cmd/generate <fixture.json> <font.ttf>
package main

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/Dickson-Pinheiro/prova-pdf/packages/go/provapdf"
)

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintf(os.Stderr, "usage: %s <fixture.json> <font.ttf>\n", os.Args[0])
		os.Exit(1)
	}

	specBytes, err := os.ReadFile(os.Args[1])
	if err != nil {
		fmt.Fprintf(os.Stderr, "read spec: %v\n", err)
		os.Exit(1)
	}

	fontBytes, err := os.ReadFile(os.Args[2])
	if err != nil {
		fmt.Fprintf(os.Stderr, "read font: %v\n", err)
		os.Exit(1)
	}

	// Unmarshal to map so GeneratePDF re-marshals the same JSON structure.
	var spec map[string]any
	if err := json.Unmarshal(specBytes, &spec); err != nil {
		fmt.Fprintf(os.Stderr, "parse spec: %v\n", err)
		os.Exit(1)
	}

	pdf, err := provapdf.GeneratePDF(spec, []provapdf.FontInput{
		{Family: "body", Variant: 0, Data: fontBytes},
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "generate: %v\n", err)
		os.Exit(1)
	}

	os.Stdout.Write(pdf)
}
