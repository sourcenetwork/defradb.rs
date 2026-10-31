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

	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
)

// Collection is a collection of a node or a transaction: the methods of Go's
// `client.Collection` Shinzo calls.
type Collection struct {
	store
	version client.CollectionVersion
}

// Name is the collection's name.
func (c *Collection) Name() string {
	return c.version.Name
}

// Version is the collection's definition.
func (c *Collection) Version() client.CollectionVersion {
	return c.version
}

// SaveDocument saves doc: updated when its DocID names a stored document,
// added otherwise.
func (c *Collection) SaveDocument(
	ctx context.Context,
	doc *client.Document,
	opts ...options.Enumerable[options.SaveDocumentOptions],
) error {
	return c.SaveManyDocuments(ctx, []*client.Document{doc}, opts...)
}

// SaveManyDocuments saves each document in order, as SaveDocument does, in
// one transaction: every document is stored or none is. A batch-signing
// collector on ctx collects the blocks it writes.
func (c *Collection) SaveManyDocuments(
	ctx context.Context,
	docs []*client.Document,
	opts ...options.Enumerable[options.SaveDocumentOptions],
) error {
	o := newOptions(opts...)
	values := make([]map[string]any, 0, len(docs))
	for _, doc := range docs {
		value, err := doc.ToMap()
		if err != nil {
			return err
		}
		// A document not saved yet has no DocID: the save gives it one.
		if doc.ID().IsValid() {
			value["_docID"] = doc.ID().String()
		} else {
			delete(value, "_docID")
		}
		values = append(values, value)
	}
	var a cargs
	defer a.free()
	docsJSON, err := a.json(values)
	if err != nil {
		return err
	}
	var optionsJSON *C.char
	if o.EncryptDoc || len(o.EncryptedFields) > 0 {
		fields := append([]string{}, o.EncryptedFields...)
		if optionsJSON, err = a.json(map[string]any{
			"encryptDoc":      o.EncryptDoc,
			"encryptedFields": fields,
		}); err != nil {
			return err
		}
	}
	raw, err := result(C.save_many_documents(
		c.node.ptr,
		a.opt(c.txn),
		a.opt(did(o.Identity)),
		a.s(c.Name()),
		docsJSON,
		optionsJSON,
		a.opt(batchSession(ctx)),
	), "save_many_documents")
	if err != nil {
		return err
	}
	var saved struct {
		DocIDs []string `json:"docIDs"`
	}
	if err := json.Unmarshal([]byte(raw), &saved); err != nil {
		return fmt.Errorf("defra: save_many_documents: %w", err)
	}
	for i, id := range saved.DocIDs {
		if i >= len(docs) {
			break
		}
		docID, err := client.NewDocIDFromString(id)
		if err != nil {
			return err
		}
		client.ApplySavedDocumentID(docs[i], docID)
	}
	return nil
}

// PurgeByDocIDs removes the documents docIDs name from this node for good,
// replicating nothing; with pruneHistory, every block of their history no
// other document owns goes too.
func (c *Collection) PurgeByDocIDs(
	ctx context.Context,
	docIDs []client.DocID,
	pruneHistory bool,
	opts ...options.Enumerable[options.TruncateCollectionOptions],
) error {
	o := newOptions(opts...)
	ids := make([]string, len(docIDs))
	for i, id := range docIDs {
		ids[i] = id.String()
	}
	var a cargs
	defer a.free()
	idsJSON, err := a.json(ids)
	if err != nil {
		return err
	}
	raw, err := result(C.purge_documents(
		c.node.ptr,
		a.opt(c.txn),
		a.opt(did(o.Identity)),
		a.s(c.Name()),
		idsJSON,
		C.bool(pruneHistory),
	), "purge_documents")
	if err != nil {
		return err
	}
	var purged struct{ Purged int }
	if err := json.Unmarshal([]byte(raw), &purged); err != nil {
		return fmt.Errorf("defra: purge_documents: %w", err)
	}
	return nil
}
