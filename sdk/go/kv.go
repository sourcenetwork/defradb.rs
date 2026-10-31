package defra

/*
#include <stdlib.h>
#include "defra.h"
*/
import "C"

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
)

// ExportDocKVs writes docIDs of the collection to w as a KV snapshot in Go
// Shinzo's format and returns the frames written. No DocIDs exports every
// document. With datastoreOnly, heads and blocks are left out; otherwise
// every earlier version and signature block is written too, so another node
// can replay the documents (Go Shinzo writes only the newest).
func (d *DB) ExportDocKVs(
	ctx context.Context,
	collectionName string,
	docIDs []string,
	w io.Writer,
	datastoreOnly bool,
) (int, error) {
	file, err := os.CreateTemp("", "defra-kv-*")
	if err != nil {
		return 0, err
	}
	defer os.Remove(file.Name())
	defer file.Close()

	var a cargs
	defer a.free()
	var ids *C.char
	if len(docIDs) > 0 {
		if ids, err = a.json(docIDs); err != nil {
			return 0, err
		}
	}
	opts, err := a.json(map[string]bool{"datastoreOnly": datastoreOnly, "history": true})
	if err != nil {
		return 0, err
	}
	raw, err := result(C.kv_export(d.ptr, nil, a.s(collectionName), ids, opts, a.s(file.Name())), "export_doc_kvs")
	if err != nil {
		return 0, err
	}
	var exported struct{ Frames int }
	if err := json.Unmarshal([]byte(raw), &exported); err != nil {
		return 0, fmt.Errorf("defra: export_doc_kvs: %w", err)
	}
	if _, err := io.Copy(w, file); err != nil {
		return 0, err
	}
	return exported.Frames, nil
}

// ExportFieldMapping is the JSON an importing node reads a snapshot of the
// collection with, Go Shinzo's shape.
func (d *DB) ExportFieldMapping(ctx context.Context, collectionName string) ([]byte, error) {
	var a cargs
	defer a.free()
	raw, err := result(C.kv_field_mapping(d.ptr, nil, a.s(collectionName)), "export_field_mapping")
	return []byte(raw), err
}

// ImportRawKVs imports the snapshot r holds and returns the frames read.
func (d *DB) ImportRawKVs(ctx context.Context, r io.Reader) (int, error) {
	return d.importKVs(r, nil)
}

// ImportRawKVsWithMapping imports the snapshot r holds, read with the source
// node's field mapping, and returns the frames read. Each document is
// replayed through the merge as a restore; one whose history the snapshot
// does not carry back to its first version is reported after the others
// are imported.
func (d *DB) ImportRawKVsWithMapping(ctx context.Context, r io.Reader, mappingJSON []byte) (int, error) {
	return d.importKVs(r, mappingJSON)
}

// RebuildCollectionIndexes does nothing: an import merges each document, and
// the merge keeps the collection's indexes.
func (d *DB) RebuildCollectionIndexes(ctx context.Context, collectionName string) error {
	return nil
}

func (d *DB) importKVs(r io.Reader, mapping []byte) (int, error) {
	file, err := os.CreateTemp("", "defra-kv-*")
	if err != nil {
		return 0, err
	}
	defer os.Remove(file.Name())
	_, err = io.Copy(file, r)
	if closeErr := file.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return 0, err
	}
	var a cargs
	defer a.free()
	var mappingJSON *C.char
	if len(mapping) > 0 {
		mappingJSON = a.s(string(mapping))
	}
	raw, err := result(C.kv_import(d.ptr, nil, a.s(file.Name()), mappingJSON), "import_raw_kvs")
	if err != nil {
		return 0, err
	}
	var imported struct{ Frames int }
	if err := json.Unmarshal([]byte(raw), &imported); err != nil {
		return 0, fmt.Errorf("defra: import_raw_kvs: %w", err)
	}
	return imported.Frames, nil
}
