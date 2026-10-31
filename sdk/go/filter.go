package defra

/*
#include <stdint.h>
*/
import "C"

import (
	"bytes"
	"context"
	"encoding/json"
	"runtime/cgo"

	"github.com/sourcenetwork/defradb/client"
)

// defraSDKReplicationFilter is the library's replication filter callback for
// a node opened with a ReplicationFilter: context is a cgo.Handle of the
// filter. The library calls it from its database workers, several at once.
//
//export defraSDKReplicationFilter
func defraSDKReplicationFilter(context C.uintptr_t, collectionID, docID, fieldsJSON *C.char) C.int {
	filter, ok := cgo.Handle(context).Value().(client.ReplicationFilter)
	if !ok {
		return 1
	}
	fields, err := decodeFields(C.GoString(fieldsJSON))
	if err != nil {
		// A filter must keep a document whose deciding field is absent.
		fields = map[string]any{}
	}
	if filter.AllowReplication(backgroundContext, C.GoString(collectionID), C.GoString(docID), fields) {
		return 1
	}
	return 0
}

// backgroundContext is the context a filter is asked in: the library's call
// carries none.
var backgroundContext = context.Background()

// decodeFields reads the fields JSON as Go DefraDB decodes a commit's CBOR
// values: a non-negative integer as uint64, a negative one as int64.
func decodeFields(raw string) (map[string]any, error) {
	decoder := json.NewDecoder(bytes.NewReader([]byte(raw)))
	decoder.UseNumber()
	var fields map[string]any
	if err := decoder.Decode(&fields); err != nil {
		return nil, err
	}
	for name, value := range fields {
		fields[name] = numbers(value)
	}
	return fields, nil
}

func numbers(value any) any {
	switch v := value.(type) {
	case json.Number:
		if u, err := v.Int64(); err == nil {
			if u >= 0 {
				return uint64(u)
			}
			return u
		}
		if f, err := v.Float64(); err == nil {
			return f
		}
		return v.String()
	case []any:
		for i := range v {
			v[i] = numbers(v[i])
		}
		return v
	case map[string]any:
		for k := range v {
			v[k] = numbers(v[k])
		}
		return v
	default:
		return v
	}
}
