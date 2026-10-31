package defra

/*
#include <stddef.h>
#include <stdint.h>
*/
import "C"

import (
	"bytes"
	"context"
	"encoding/json"
	"runtime/cgo"
	"unsafe"

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

// defraSDKRetentionRule is the library's retention callback: the Go
// `client.RetentionRule` the handle context names gives the collection's
// height field and floor. A floor of zero or less refuses nothing.
//
//export defraSDKRetentionRule
func defraSDKRetentionRule(
	context C.uintptr_t,
	collectionID *C.char,
	fieldOut *C.char,
	fieldCapacity C.size_t,
	floorOut *C.int64_t,
) C.int {
	rule, ok := cgo.Handle(context).Value().(client.RetentionRule)
	if !ok {
		return 0
	}
	field, floor, covered := rule.RetentionFloor(C.GoString(collectionID))
	if !covered || floor <= 0 || len(field) >= int(fieldCapacity) {
		return 0
	}
	out := unsafe.Slice((*byte)(unsafe.Pointer(fieldOut)), int(fieldCapacity))
	copy(out, field)
	out[len(field)] = 0
	*floorOut = C.int64_t(floor)
	return 1
}
