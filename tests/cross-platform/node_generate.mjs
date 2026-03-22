// node_generate.mjs — generates a PDF via the browser wasm-bindgen package.
//
// Usage: node node_generate.mjs <fixture.json> <font.ttf>
import { readFileSync } from "fs";
import { initSync, add_font, generate_pdf, clear_all } from "../../pkg/prova_pdf.js";

const [,, fixturePath, fontPath] = process.argv;
if (!fixturePath || !fontPath) {
  process.stderr.write("usage: node node_generate.mjs <fixture.json> <font.ttf>\n");
  process.exit(1);
}

// Init WASM synchronously
const wasmBytes = readFileSync(new URL("../../pkg/prova_pdf_bg.wasm", import.meta.url));
initSync({ module: wasmBytes });

clear_all();

// Register font
const fontBytes = readFileSync(fontPath);
add_font("body", 0, new Uint8Array(fontBytes));

// Generate PDF
const spec = JSON.parse(readFileSync(fixturePath, "utf-8"));
const pdf = generate_pdf(spec);

process.stdout.write(Buffer.from(pdf));
