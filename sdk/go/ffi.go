// Package defra is a Go SDK for defradb.rs: it opens a Rust node in-process
// through the C ABI of `crates/ffi` and offers Go Shinzo's client methods on
// it, with the names and argument types of Go DefraDB's `client` package at
// shinzo-develop, so a Shinzo program switches to it mostly by its import.
//
// Build: the C library and its header come from this repository.
//
//	cargo build -p ffi --profile super-dev
//	cbindgen --config crates/ffi/cbindgen.toml --crate ffi --output crates/ffi/defra.h
//	CGO_LDFLAGS="-L$PWD/target/super-dev -Wl,-rpath,$PWD/target/super-dev" go test ./...
//
// The library is libffi from that target directory, so -L and the rpath must
// name it ahead of the system's libffi.
package defra

/*
#cgo CFLAGS: -I${SRCDIR}/../../crates/ffi
#cgo LDFLAGS: -lffi -ldl -lpthread -lm
#include <stdlib.h>
#include "defra.h"
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"unsafe"

	"github.com/sourcenetwork/defradb/client"
)

var initOnce sync.Once

// initFFI starts the library's runtime once per process.
func initFFI() {
	initOnce.Do(func() { C.defra_init() })
}

// cargs holds the C strings one call passes; free releases them all.
type cargs struct {
	p []unsafe.Pointer
}

// s is v as a C string that lives until free.
func (a *cargs) s(v string) *C.char {
	c := C.CString(v)
	a.p = append(a.p, unsafe.Pointer(c))
	return c
}

// opt is s, except that an empty string is a null pointer: the library reads
// null as an argument not given.
func (a *cargs) opt(v string) *C.char {
	if v == "" {
		return nil
	}
	return a.s(v)
}

// json is v marshalled as a C string.
func (a *cargs) json(v any) (*C.char, error) {
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, err
	}
	return a.s(string(raw)), nil
}

func (a *cargs) free() {
	for _, p := range a.p {
		C.free(p)
	}
	a.p = nil
}

// result turns an FfiResult into its value or its error, freeing both.
func result(r C.struct_FfiResult, op string) (string, error) {
	if r.status != 0 {
		message := C.GoString(r.error)
		C.defra_free_string(r.error)
		return "", ffiError(op, message)
	}
	if r.value == nil {
		return "", nil
	}
	value := C.GoString(r.value)
	C.defra_free_string(r.value)
	return value, nil
}

// ffiError is the library's error message, wrapping the `client` sentinel
// errors Go DefraDB returns for it, so errors.Is matches as it does there.
func ffiError(op, message string) error {
	switch {
	case strings.Contains(message, "not authorized to perform operation"):
		return fmt.Errorf("%w: %s", client.ErrNotAuthorizedToPerformOperation, message)
	case strings.Contains(message, "document not found or not authorized"):
		return fmt.Errorf("%w: %s", client.ErrDocumentNotFoundOrNotAuthorized, message)
	case strings.Contains(message, "collection not found"),
		strings.Contains(message, "collection '") && strings.Contains(message, "' not found"):
		return fmt.Errorf("%w: %s", client.ErrCollectionNotFound, message)
	default:
		return fmt.Errorf("defra: %s: %s", op, message)
	}
}
