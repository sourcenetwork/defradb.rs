package defra

/*
#include <stdlib.h>
#include "defra.h"
*/
import "C"

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"

	"github.com/ipfs/go-cid"
	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
	"github.com/sourcenetwork/defradb/node"
	lensmodel "github.com/sourcenetwork/lens/host-go/config/model"
)

// store is what a database and a transaction both offer, Go's `client.Store`,
// in the transaction txn names when it is set. Methods the Rust SDK does not
// offer return ErrNotSupported.
type store struct {
	ptr C.uintptr_t
	txn string
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
		raw, err = result(C.add_schema_in_txn(s.ptr, a.s(s.txn), a.opt(did(o.Identity)), a.s(sdl)), "add_collection")
	} else {
		raw, err = result(C.add_schema(s.ptr, a.opt(did(o.Identity)), a.s(sdl)), "add_collection")
	}
	if err != nil {
		return nil, err
	}
	return decode[[]client.CollectionVersion](raw, "add_collection")
}

// ExecRequest runs a GraphQL request. Go DefraDB's batch-signing collector
// on ctx collects the composite blocks it writes.
func (s *store) ExecRequest(
	ctx context.Context,
	request string,
	opts ...options.Enumerable[options.ExecRequestOptions],
) *client.RequestResult {
	o := newOptions(opts...)
	operation := ""
	if o.OperationName.HasValue() {
		operation = o.OperationName.Value()
	}
	return s.execOperation(ctx, request, operation, did(o.Identity), o.Variables)
}

// exec runs request as identityDID with variables.
func (s *store) exec(ctx context.Context, request string, identityDID string, variables map[string]any) *client.RequestResult {
	return s.execOperation(ctx, request, "", identityDID, variables)
}

func (s *store) execOperation(
	ctx context.Context,
	request string,
	operation string,
	identityDID string,
	variables map[string]any,
) *client.RequestResult {
	var a cargs
	defer a.free()
	var vars *C.char
	if len(variables) > 0 {
		v, err := a.json(variables)
		if err != nil {
			return errResult(err)
		}
		vars = v
	}
	var raw string
	err := collect(ctx, func(session *C.char) error {
		var err error
		raw, err = result(C.exec_request(
			s.ptr, a.opt(s.txn), a.opt(identityDID), a.s(request), a.opt(operation), vars, session,
		), "exec_request")
		return err
	})
	if err != nil {
		return errResult(err)
	}
	var response struct {
		Data   any `json:"data"`
		Errors []struct {
			Message string `json:"message"`
		} `json:"errors"`
	}
	if err := json.Unmarshal([]byte(raw), &response); err != nil {
		return errResult(err)
	}
	out := &client.RequestResult{GQL: client.GQLResult{Data: response.Data}}
	for _, e := range response.Errors {
		out.GQL.Errors = append(out.GQL.Errors, fmt.Errorf("%s", e.Message))
	}
	return out
}

func errResult(err error) *client.RequestResult {
	return &client.RequestResult{GQL: client.GQLResult{Errors: []error{err}}}
}

// GetCollectionByName is the active collection named name.
func (s *store) GetCollectionByName(
	ctx context.Context,
	name client.CollectionName,
	opts ...options.Enumerable[options.GetCollectionByNameOptions],
) (client.Collection, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	raw, err := result(C.get_collection_by_name(s.ptr, a.opt(did(o.Identity)), a.s(string(name))), "get_collection_by_name")
	if err != nil {
		return nil, err
	}
	version, err := decode[client.CollectionVersion](raw, "get_collection_by_name")
	if err != nil {
		return nil, err
	}
	return &Collection{st: *s, version: version}, nil
}

// GetCollections is the collections the options select: active ones unless
// GetInactive, narrowed by name, collection ID, version ID and set ID.
func (s *store) GetCollections(
	ctx context.Context,
	opts ...options.Enumerable[options.GetCollectionsOptions],
) ([]client.Collection, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	var raw string
	var err error
	if s.txn != "" {
		raw, err = result(C.get_collections_in_txn(s.ptr, a.s(s.txn), a.opt(did(o.Identity))), "get_collections")
	} else {
		raw, err = result(C.get_collections(s.ptr, a.opt(did(o.Identity))), "get_collections")
	}
	if err != nil {
		return nil, err
	}
	versions, err := decode[[]client.CollectionVersion](raw, "get_collections")
	if err != nil {
		return nil, err
	}
	inactive := o.GetInactive.HasValue() && o.GetInactive.Value()
	var out []client.Collection
	for _, v := range versions {
		switch {
		case !inactive && !v.IsActive && !o.VersionID.HasValue(),
			o.CollectionName.HasValue() && v.Name != o.CollectionName.Value(),
			o.CollectionID.HasValue() && v.CollectionID != o.CollectionID.Value(),
			o.VersionID.HasValue() && v.VersionID != o.VersionID.Value(),
			o.CollectionSetID.HasValue() &&
				(!v.CollectionSet.HasValue() || v.CollectionSet.Value().CollectionSetID != o.CollectionSetID.Value()):
			continue
		}
		out = append(out, &Collection{st: *s, version: v})
	}
	return out, nil
}

// AddView adds a view: the collection sdl describes, filled by gqlQuery and
// the lens the options' TransformCID names.
func (s *store) AddView(
	ctx context.Context,
	gqlQuery string,
	sdl string,
	opts ...options.Enumerable[options.AddViewOptions],
) ([]client.CollectionVersion, error) {
	if s.txn != "" {
		return nil, errNotSupported("AddView in a transaction")
	}
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	var transform *C.char
	if o.TransformCID.HasValue() {
		transform = a.s(o.TransformCID.Value())
	}
	raw, err := result(C.add_view(s.ptr, a.opt(did(o.Identity)), a.s(gqlQuery), a.s(sdl), transform), "add_view")
	if err != nil {
		return nil, err
	}
	return decode[[]client.CollectionVersion](raw, "add_view")
}

// AddLens stores lens and returns its CID.
func (s *store) AddLens(
	ctx context.Context,
	lens lensmodel.Lens,
	opts ...options.Enumerable[options.AddLensOptions],
) (string, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	lensJSON, err := a.json(lens)
	if err != nil {
		return "", err
	}
	var raw string
	if s.txn != "" {
		raw, err = result(C.lens_add_in_txn(s.ptr, a.s(s.txn), a.opt(did(o.Identity)), lensJSON), "add_lens")
	} else {
		raw, err = result(C.lens_add(s.ptr, a.opt(did(o.Identity)), lensJSON), "add_lens")
	}
	if err != nil {
		return "", err
	}
	if id, err := decode[string](raw, "add_lens"); err == nil {
		return id, nil
	}
	return raw, nil
}

// decode is raw unmarshalled as T.
func decode[T any](raw string, op string) (T, error) {
	var v T
	if err := json.Unmarshal([]byte(raw), &v); err != nil {
		return v, fmt.Errorf("defra: %s: %w", op, err)
	}
	return v, nil
}

// collect runs write; when ctx carries Go DefraDB's batch-signing collector
// it runs it in a batch session of its own and adds the composite blocks it
// wrote to that collector, as a Go node's writes do.
func collect(ctx context.Context, write func(session *C.char) error) error {
	collector := node.BatchSigningCollectorFromContext(ctx)
	if collector == nil {
		return write(nil)
	}
	id := make([]byte, 16)
	if _, err := rand.Read(id); err != nil {
		return err
	}
	var a cargs
	defer a.free()
	session := a.s(hex.EncodeToString(id))
	if _, err := result(C.batch_collect_start(session, nil), "batch_collect"); err != nil {
		return err
	}
	written := write(session)
	raw, err := result(C.batch_collect_take(session), "batch_collect")
	if written != nil {
		return written
	}
	if err != nil {
		return err
	}
	cids, err := decode[[]string](raw, "batch_collect")
	if err != nil {
		return err
	}
	for _, s := range cids {
		id, err := cid.Decode(s)
		if err != nil {
			return fmt.Errorf("defra: batch_collect: %w", err)
		}
		collector.Add(id)
	}
	return nil
}
