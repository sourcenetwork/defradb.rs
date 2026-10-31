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
	"strings"

	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
)

// Collection is one version of a collection, read and written through the
// store it came from: Go's `client.Collection`. Methods the Rust SDK does
// not offer return ErrNotSupported.
type Collection struct {
	st      store
	version client.CollectionVersion
}

var _ client.Collection = (*Collection)(nil)

// Name is the collection's name.
func (c *Collection) Name() string { return c.version.Name }

// Version is the collection version this handle reads and writes.
func (c *Collection) Version() client.CollectionVersion { return c.version }

// VersionID is the collection version's ID.
func (c *Collection) VersionID() string { return c.version.VersionID }

// CollectionID is the ID every version of the collection shares.
func (c *Collection) CollectionID() string { return c.version.CollectionID }

// AddDocument adds doc, which must not exist yet, and sets its saved DocID.
func (c *Collection) AddDocument(
	ctx context.Context,
	doc *client.Document,
	opts ...options.Enumerable[options.AddDocumentOptions],
) error {
	return c.AddManyDocuments(ctx, []*client.Document{doc}, opts...)
}

// AddManyDocuments adds docs in one request, none of which may exist yet.
func (c *Collection) AddManyDocuments(
	ctx context.Context,
	docs []*client.Document,
	opts ...options.Enumerable[options.AddDocumentOptions],
) error {
	o := newOptions(opts...)
	inputs := make([]json.RawMessage, len(docs))
	for i, doc := range docs {
		raw, err := doc.ToJSONPatch()
		if err != nil {
			return err
		}
		inputs[i] = raw
	}
	args := "input: $input"
	if o.EncryptDoc {
		args += ", encrypt: true"
	}
	if len(o.EncryptedFields) > 0 {
		fields, err := json.Marshal(o.EncryptedFields)
		if err != nil {
			return err
		}
		args += ", encryptFields: " + string(fields)
	}
	name := c.Name()
	request := fmt.Sprintf(`mutation($input: [%sMutationInputArg!]) { add_%s(%s) { _docID } }`, name, name, args)
	res := c.st.exec(ctx, request, did(o.Identity), map[string]any{"input": inputs})
	if len(res.GQL.Errors) > 0 {
		return res.GQL.Errors[0]
	}
	ids := docIDs(res.GQL.Data, "add_"+name)
	if len(ids) != len(docs) {
		return fmt.Errorf("defra: add_%s: %d documents added, %d returned", name, len(docs), len(ids))
	}
	for i, id := range ids {
		docID, err := client.NewDocIDFromString(id)
		if err != nil {
			return err
		}
		client.ApplySavedDocumentID(docs[i], docID)
		docs[i].Clean()
	}
	return nil
}

// UpdateDocument writes doc's values over the document its DocID names.
func (c *Collection) UpdateDocument(
	ctx context.Context,
	doc *client.Document,
	opts ...options.Enumerable[options.UpdateDocumentOptions],
) error {
	o := newOptions(opts...)
	return c.save(ctx, []*client.Document{doc}, did(o.Identity), false, nil)
}

// SaveDocument adds doc, or updates it when it exists.
func (c *Collection) SaveDocument(
	ctx context.Context,
	doc *client.Document,
	opts ...options.Enumerable[options.SaveDocumentOptions],
) error {
	return c.SaveManyDocuments(ctx, []*client.Document{doc}, opts...)
}

// SaveManyDocuments saves docs in order in one transaction: every document
// is stored or none is.
func (c *Collection) SaveManyDocuments(
	ctx context.Context,
	docs []*client.Document,
	opts ...options.Enumerable[options.SaveDocumentOptions],
) error {
	o := newOptions(opts...)
	return c.save(ctx, docs, did(o.Identity), o.EncryptDoc, o.EncryptedFields)
}

func (c *Collection) save(
	ctx context.Context,
	docs []*client.Document,
	identityDID string,
	encryptDoc bool,
	encryptedFields []string,
) error {
	values := make([]map[string]any, 0, len(docs))
	for _, doc := range docs {
		value, err := doc.ToMap()
		if err != nil {
			return err
		}
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
	if encryptDoc || len(encryptedFields) > 0 {
		if optionsJSON, err = a.json(map[string]any{
			"encryptDoc":      encryptDoc,
			"encryptedFields": append([]string{}, encryptedFields...),
		}); err != nil {
			return err
		}
	}
	var raw string
	err = collect(ctx, func(session *C.char) error {
		var err error
		raw, err = result(C.save_many_documents(
			c.st.ptr, a.opt(c.st.txn), a.opt(identityDID), a.s(c.Name()), docsJSON, optionsJSON, session,
		), "save_many_documents")
		return err
	})
	if err != nil {
		return err
	}
	saved, err := decode[struct {
		DocIDs []string `json:"docIDs"`
	}](raw, "save_many_documents")
	if err != nil {
		return err
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
		docs[i].Clean()
	}
	return nil
}

// GetDocument is the document docID names, with its scalar fields.
func (c *Collection) GetDocument(
	ctx context.Context,
	docID client.DocID,
	opts ...options.Enumerable[options.GetDocumentOptions],
) (*client.Document, error) {
	o := newOptions(opts...)
	var fields []string
	for _, field := range c.version.Fields {
		if field.Kind.IsObject() || strings.HasPrefix(field.Name, "_") {
			continue
		}
		fields = append(fields, field.Name)
	}
	args := fmt.Sprintf(`docID: %q`, docID.String())
	if o.ShowDeleted {
		args += ", showDeleted: true"
	}
	request := fmt.Sprintf(`query { %s(%s) { _docID %s } }`, c.Name(), args, strings.Join(fields, " "))
	res := c.st.exec(ctx, request, did(o.Identity), nil)
	if len(res.GQL.Errors) > 0 {
		return nil, res.GQL.Errors[0]
	}
	data, _ := res.GQL.Data.(map[string]any)
	rows, _ := data[c.Name()].([]any)
	if len(rows) == 0 {
		return nil, client.ErrDocumentNotFoundOrNotAuthorized
	}
	row, _ := rows[0].(map[string]any)
	delete(row, "_docID")
	raw, err := json.Marshal(row)
	if err != nil {
		return nil, err
	}
	doc, err := client.NewDocWithID(ctx, docID, c.version)
	if err != nil {
		return nil, err
	}
	if err := doc.SetWithJSON(ctx, raw); err != nil {
		return nil, err
	}
	doc.Clean()
	return doc, nil
}

// DeleteDocumentsWithFilter deletes the documents filter matches.
func (c *Collection) DeleteDocumentsWithFilter(
	ctx context.Context,
	filter any,
	opts ...options.Enumerable[options.DeleteDocumentsWithFilterOptions],
) (*client.DeleteResult, error) {
	o := newOptions(opts...)
	if text, ok := filter.(string); ok {
		if err := json.Unmarshal([]byte(text), &filter); err != nil {
			return nil, fmt.Errorf("defra: delete filter: %w", err)
		}
	}
	name := c.Name()
	request := fmt.Sprintf(`mutation($filter: %sFilterArg) { delete_%s(filter: $filter) { _docID } }`, name, name)
	res := c.st.exec(ctx, request, did(o.Identity), map[string]any{"filter": filter})
	if len(res.GQL.Errors) > 0 {
		return nil, res.GQL.Errors[0]
	}
	ids := docIDs(res.GQL.Data, "delete_"+name)
	return &client.DeleteResult{Count: int64(len(ids)), DocIDs: ids}, nil
}

// PurgeByDocIDs removes docIDs from the node in one transaction, history and
// all when pruneHistory is set; nothing is replicated.
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
	_, err = result(C.purge_documents(
		c.st.ptr, a.opt(c.st.txn), a.opt(did(o.Identity)), a.s(c.Name()), idsJSON, C.bool(pruneHistory),
	), "purge_documents")
	return err
}

// Truncate removes every document of the collection from the node.
func (c *Collection) Truncate(ctx context.Context, opts ...options.Enumerable[options.TruncateCollectionOptions]) error {
	if c.st.txn != "" {
		return errNotSupported("Truncate in a transaction")
	}
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	_, err := result(C.truncate_collection(c.st.ptr, a.opt(did(o.Identity)), a.s(c.Name())), "truncate")
	return err
}

// docIDs is the `_docID` of each row data[key] holds.
func docIDs(data any, key string) []string {
	m, _ := data.(map[string]any)
	rows, _ := m[key].([]any)
	ids := make([]string, 0, len(rows))
	for _, r := range rows {
		if row, ok := r.(map[string]any); ok {
			if id, ok := row["_docID"].(string); ok {
				ids = append(ids, id)
			}
		}
	}
	return ids
}
