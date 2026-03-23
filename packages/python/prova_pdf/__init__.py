"""prova-pdf: Exam PDF generator powered by WebAssembly."""

from __future__ import annotations

__version__ = "0.1.3"
__all__ = ["generate_pdf", "FontInput", "ProvaPdfError"]

import ctypes
import json
from pathlib import Path
from typing import Any, TypedDict

import wasmtime


class FontInput(TypedDict):
    """Font registration input."""

    family: str
    variant: int  # 0=regular, 1=bold, 2=italic, 3=bold-italic
    data: bytes


class ProvaPdfError(Exception):
    """Raised when the WASM module reports an error."""


def _find_wasm() -> Path:
    """Locate prova_pdf.wasm -- bundled in package or in repo wasm/ dir."""
    pkg_dir = Path(__file__).parent
    candidates = [
        pkg_dir / "prova_pdf.wasm",
        pkg_dir.parent.parent.parent / "wasm" / "prova_pdf.wasm",
    ]
    for p in candidates:
        if p.exists():
            return p
    raise FileNotFoundError(
        "prova_pdf.wasm not found. Run `make build-wasi` first."
    )


class _Runtime:
    """Lazy-loaded WASI runtime singleton."""

    _instance: _Runtime | None = None

    @classmethod
    def get(cls) -> _Runtime:
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    def __init__(self) -> None:
        wasm_path = _find_wasm()
        engine = wasmtime.Engine()
        self._store = wasmtime.Store(engine)
        linker = wasmtime.Linker(engine)
        linker.define_wasi()
        wasi_config = wasmtime.WasiConfig()
        self._store.set_wasi(wasi_config)
        module = wasmtime.Module.from_file(engine, str(wasm_path))
        instance = linker.instantiate(self._store, module)

        exports = instance.exports(self._store)
        self._memory: wasmtime.Memory = exports["memory"]

        # Cache exported functions
        self._alloc = exports["prova_pdf_alloc"]
        self._free = exports["prova_pdf_free"]
        self._add_font = exports["prova_pdf_add_font"]
        self._set_font_rules = exports["prova_pdf_set_font_rules"]
        self._add_image = exports["prova_pdf_add_image"]
        self._clear_all = exports["prova_pdf_clear_all"]
        self._generate = exports["prova_pdf_generate"]
        self._output_ptr = exports["prova_pdf_output_ptr"]
        self._output_len = exports["prova_pdf_output_len"]
        self._last_error_len = exports["prova_pdf_last_error_len"]
        self._last_error_message = exports["prova_pdf_last_error_message"]

    # -- memory helpers ------------------------------------------------

    def _mem_base(self) -> int:
        """Return the integer base address of WASM linear memory."""
        raw = self._memory.data_ptr(self._store)
        return ctypes.cast(raw, ctypes.c_void_p).value or 0

    def _write_bytes(self, data: bytes) -> tuple[int, int]:
        """Allocate WASM memory and write *data*. Returns (ptr, len)."""
        n = len(data)
        if n == 0:
            return 0, 0
        ptr: int = self._alloc(self._store, n)
        base = self._mem_base()
        src = (ctypes.c_ubyte * n).from_buffer_copy(data)
        ctypes.memmove(base + ptr, src, n)
        return ptr, n

    def _read_bytes(self, ptr: int, n: int) -> bytes:
        """Read *n* bytes from WASM memory at *ptr*."""
        if n == 0:
            return b""
        base = self._mem_base()
        return bytes((ctypes.c_ubyte * n).from_address(base + ptr))

    def _free_pair(self, ptr: int, length: int) -> None:
        """Free a previously allocated region."""
        if length > 0:
            self._free(self._store, ptr, length)

    # -- error handling ------------------------------------------------

    def _read_last_error(self) -> str:
        """Read the last error message from the WASM module."""
        err_len: int = self._last_error_len(self._store)
        if err_len == 0:
            return "unknown error"
        buf_ptr: int = self._alloc(self._store, err_len)
        self._last_error_message(self._store, buf_ptr)
        data = self._read_bytes(buf_ptr, err_len)
        self._free(self._store, buf_ptr, err_len)
        return data.decode("utf-8", errors="replace")

    def _check(self, rc: int) -> int:
        """Raise :class:`ProvaPdfError` if *rc* < 0."""
        if rc < 0:
            raise ProvaPdfError(self._read_last_error())
        return rc

    # -- public operations ---------------------------------------------

    def clear_all(self) -> None:
        self._clear_all(self._store)

    def add_font(self, family: str, variant: int, data: bytes) -> None:
        family_bytes = family.encode("utf-8")
        fam_ptr, fam_len = self._write_bytes(family_bytes)
        dat_ptr, dat_len = self._write_bytes(data)
        try:
            rc: int = self._add_font(
                self._store, fam_ptr, fam_len, variant, dat_ptr, dat_len
            )
            self._check(rc)
        finally:
            self._free_pair(fam_ptr, fam_len)
            self._free_pair(dat_ptr, dat_len)

    def set_font_rules(self, rules: dict[str, Any]) -> None:
        payload = json.dumps(rules).encode("utf-8")
        ptr, length = self._write_bytes(payload)
        try:
            rc: int = self._set_font_rules(self._store, ptr, length)
            self._check(rc)
        finally:
            self._free_pair(ptr, length)

    def add_image(self, key: str, data: bytes) -> None:
        key_bytes = key.encode("utf-8")
        key_ptr, key_len = self._write_bytes(key_bytes)
        dat_ptr, dat_len = self._write_bytes(data)
        try:
            rc: int = self._add_image(
                self._store, key_ptr, key_len, dat_ptr, dat_len
            )
            self._check(rc)
        finally:
            self._free_pair(key_ptr, key_len)
            self._free_pair(dat_ptr, dat_len)

    def generate(self, spec_json: bytes) -> bytes:
        """Run the two-call generate protocol and return PDF bytes."""
        ptr, length = self._write_bytes(spec_json)
        try:
            # First call: out_buf=0, out_cap=0 -> stages output internally
            rc: int = self._generate(self._store, ptr, length, 0, 0)
            self._check(rc)
        finally:
            self._free_pair(ptr, length)

        # Read staged output
        out_ptr: int = self._output_ptr(self._store)
        out_len: int = self._output_len(self._store)
        if out_len == 0:
            raise ProvaPdfError("generate produced zero-length output")
        return self._read_bytes(out_ptr, out_len)


# -- public API --------------------------------------------------------


def generate_pdf(
    spec: dict[str, Any],
    fonts: list[FontInput],
    images: dict[str, bytes] | None = None,
    font_rules: dict[str, Any] | None = None,
) -> bytes:
    """Generate a PDF from *spec*.

    Parameters
    ----------
    spec:
        The exam specification dict (serialised to JSON internally).
    fonts:
        List of fonts to register before generation.
    images:
        Optional mapping of image key -> raw image bytes.
    font_rules:
        Optional font-rule configuration dict.

    Returns
    -------
    bytes
        The raw PDF file content.

    Raises
    ------
    ProvaPdfError
        If the WASM module reports an error at any step.
    FileNotFoundError
        If prova_pdf.wasm cannot be located.
    """
    rt = _Runtime.get()

    # 1. Clean slate
    rt.clear_all()

    # 2. Register fonts
    for font in fonts:
        rt.add_font(font["family"], font["variant"], font["data"])

    # 3. Register images
    if images:
        for key, data in images.items():
            rt.add_image(key, data)

    # 4. Font rules
    if font_rules is not None:
        rt.set_font_rules(font_rules)

    # 5. Generate
    spec_bytes = json.dumps(spec).encode("utf-8")
    return rt.generate(spec_bytes)
