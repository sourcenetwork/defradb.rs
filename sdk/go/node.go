package defra

/*
#include <stdlib.h>
#include "defra.h"
extern int defraSDKReplicationFilterShim(uintptr_t context, const char *collection_id,
                                         const char *doc_id, const char *fields_json);
*/
import "C"

import (
	"context"
	"encoding/json"
	"fmt"
	"runtime/cgo"
	"strconv"

	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
)

// Node is a defradb.rs node opened in this process: Go Shinzo's `node.Node`
// with its `DB`. It is safe for concurrent use.
type Node struct {
	store
	ptr    C.uintptr_t
	filter cgo.Handle
}

// Open opens a node with opts.
func Open(ctx context.Context, opts Options) (*Node, error) {
	initFFI()
	var a cargs
	defer a.free()

	var c C.struct_NodeInitOptions
	if opts.Path != "" {
		c.db_path = a.s(opts.Path)
	} else {
		c.in_memory = 1
	}
	c.iroh_discovery = 1
	if opts.EnableSigning {
		c.enable_signing = 1
	}
	if opts.ShinzoWire {
		c.shinzo_wire = 1
	}
	if len(opts.RetentionFloors) > 0 {
		floors, err := a.json(opts.RetentionFloors)
		if err != nil {
			return nil, err
		}
		c.retention_json = floors
	}
	var filter cgo.Handle
	if opts.ReplicationFilter != nil {
		filter = cgo.NewHandle(opts.ReplicationFilter)
		c.replication_filter = (*[0]byte)(C.defraSDKReplicationFilterShim)
		c.replication_filter_context = C.uintptr_t(filter)
	}

	var r C.struct_NewNodeResult
	if opts.ListenAddress != "" {
		r = C.new_node_with_p2p(c, a.s(opts.ListenAddress))
	} else {
		r = C.new_node(c)
	}
	if r.status != 0 {
		message := C.GoString(r.error)
		C.defra_free_string(r.error)
		if filter != 0 {
			filter.Delete()
		}
		return nil, ffiError("open", message)
	}
	n := &Node{ptr: r.node_ptr, filter: filter}
	n.store = store{node: n}
	return n, nil
}

// Close closes the node.
func (n *Node) Close() {
	_, _ = result(C.node_close(n.ptr), "close")
	if n.filter != 0 {
		n.filter.Delete()
		n.filter = 0
	}
}

// NewTxn begins a transaction; its writes land when it commits.
func (n *Node) NewTxn(readOnly bool) (*Txn, error) {
	readonly := C.int32_t(0)
	if readOnly {
		readonly = 1
	}
	r := C.begin_txn(n.ptr, readonly)
	if r.status != 0 {
		message := C.GoString(r.error)
		C.defra_free_string(r.error)
		return nil, ffiError("new_txn", message)
	}
	id := C.GoString(r.txn_id)
	C.defra_free_string(r.txn_id)
	t := &Txn{id: id}
	t.store = store{node: n, txn: id}
	return t, nil
}

// Txn is a transaction: Go's `client.Txn`, a store whose writes land when
// it commits.
type Txn struct {
	store
	id string
}

// ID is the transaction's identifier.
func (t *Txn) ID() uint64 {
	id, _ := strconv.ParseUint(t.id, 10, 64)
	return id
}

// Commit commits the transaction.
func (t *Txn) Commit() error {
	var a cargs
	defer a.free()
	_, err := result(C.commit_txn(t.node.ptr, a.s(t.id)), "commit")
	return err
}

// Discard discards the transaction: nothing it wrote lands.
func (t *Txn) Discard() {
	var a cargs
	defer a.free()
	_, _ = result(C.rollback_txn(t.node.ptr, a.s(t.id)), "discard")
}

// store is what a node and a transaction both offer: Go's `client.Store`
// methods Shinzo calls, in the transaction `txn` names when it is set.
type store struct {
	node *Node
	txn  string
}

// AddCollection adds the collections the GraphQL SDL describes.
func (s *store) AddCollection(
	ctx context.Context,
	sdl string,
	opts ...options.Enumerable[options.AddCollectionOptions],
) ([]client.CollectionVersion, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	var raw string
	var err error
	if s.txn != "" {
		raw, err = result(C.add_schema_in_txn(s.node.ptr, a.s(s.txn), a.opt(did(o.Identity)), a.s(sdl)), "add_collection")
	} else {
		raw, err = result(C.add_schema(s.node.ptr, a.opt(did(o.Identity)), a.s(sdl)), "add_collection")
	}
	if err != nil {
		return nil, err
	}
	var versions []client.CollectionVersion
	if err := json.Unmarshal([]byte(raw), &versions); err != nil {
		return nil, fmt.Errorf("defra: add_collection: %w", err)
	}
	return versions, nil
}

// ExecRequest runs a GraphQL request. A batch-signing collector on ctx
// collects the blocks it writes.
func (s *store) ExecRequest(
	ctx context.Context,
	request string,
	opts ...options.Enumerable[options.ExecRequestOptions],
) *client.RequestResult {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	var operation, variables *C.char
	if o.OperationName.HasValue() {
		operation = a.s(o.OperationName.Value())
	}
	if len(o.Variables) > 0 {
		vars, err := a.json(o.Variables)
		if err != nil {
			return &client.RequestResult{GQL: client.GQLResult{Errors: []error{err}}}
		}
		variables = vars
	}
	raw, err := result(C.exec_request(
		s.node.ptr,
		a.opt(s.txn),
		a.opt(did(o.Identity)),
		a.s(request),
		operation,
		variables,
		a.opt(batchSession(ctx)),
	), "exec_request")
	if err != nil {
		return &client.RequestResult{GQL: client.GQLResult{Errors: []error{err}}}
	}
	var response struct {
		Data   any `json:"data"`
		Errors []struct {
			Message string `json:"message"`
		} `json:"errors"`
	}
	if err := json.Unmarshal([]byte(raw), &response); err != nil {
		return &client.RequestResult{GQL: client.GQLResult{Errors: []error{err}}}
	}
	out := &client.RequestResult{GQL: client.GQLResult{Data: response.Data}}
	for _, e := range response.Errors {
		out.GQL.Errors = append(out.GQL.Errors, fmt.Errorf("%s", e.Message))
	}
	return out
}

// GetCollectionByName is the collection named name, in this store.
func (s *store) GetCollectionByName(
	ctx context.Context,
	name client.CollectionName,
	opts ...options.Enumerable[options.GetCollectionByNameOptions],
) (*Collection, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	raw, err := result(C.get_collection_by_name(s.node.ptr, a.opt(did(o.Identity)), a.s(string(name))), "get_collection_by_name")
	if err != nil {
		return nil, err
	}
	var version client.CollectionVersion
	if err := json.Unmarshal([]byte(raw), &version); err != nil {
		return nil, fmt.Errorf("defra: get_collection_by_name: %w", err)
	}
	return &Collection{store: *s, version: version}, nil
}
