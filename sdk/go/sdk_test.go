package defra

import (
	"bytes"
	"context"
	"encoding/json"

	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
)

const users = "type User { name: String @index age: Int }"

func open(t *testing.T, opts Options) *Node {
	t.Helper()
	node, err := Open(context.Background(), opts)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(node.Close)
	if _, err := node.AddCollection(context.Background(), users); err != nil {
		t.Fatal(err)
	}
	return node
}

type execer interface {
	ExecRequest(context.Context, string, ...options.Enumerable[options.ExecRequestOptions]) *client.RequestResult
}

// query runs request and returns its data as JSON.
func query(t *testing.T, s execer, request string) string {
	t.Helper()
	result := s.ExecRequest(context.Background(), request)
	if len(result.GQL.Errors) > 0 {
		t.Fatal(result.GQL.Errors)
	}
	raw, err := json.Marshal(result.GQL.Data)
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

func names(t *testing.T, s execer) string {
	return query(t, s, `query { User(order: {name: ASC}) { name age } }`)
}

func docs(t *testing.T, collection *Collection, values ...string) []*client.Document {
	t.Helper()
	var out []*client.Document
	for _, value := range values {
		doc, err := client.NewDocFromJSON(context.Background(), []byte(value), collection.Version())
		if err != nil {
			t.Fatal(err)
		}
		out = append(out, doc)
	}
	return out
}

func collectionOf(t *testing.T, s interface {
	GetCollectionByName(context.Context, client.CollectionName, ...options.Enumerable[options.GetCollectionByNameOptions]) (*Collection, error)
}) *Collection {
	t.Helper()
	collection, err := s.GetCollectionByName(context.Background(), "User")
	if err != nil {
		t.Fatal(err)
	}
	return collection
}

func TestExecRequestAndSaveManyDocuments(t *testing.T) {
	node := open(t, Options{})
	ctx := context.Background()
	collection := collectionOf(t, node)
	if collection.Name() != "User" {
		t.Fatalf("name %q", collection.Name())
	}
	saved := docs(t, collection, `{"name": "Alice", "age": 40}`, `{"name": "Bob", "age": 7}`)
	if err := collection.SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}
	if got := names(t, node); got != `{"User":[{"age":40,"name":"Alice"},{"age":7,"name":"Bob"}]}` {
		t.Fatal(got)
	}
	if !saved[0].ID().IsValid() {
		t.Fatal("the save gives a new document its DocID")
	}
	// Saving a document whose DocID is stored updates it.
	if err := saved[0].Set(ctx, "age", 41); err != nil {
		t.Fatal(err)
	}
	if err := collection.SaveDocument(ctx, saved[0]); err != nil {
		t.Fatal(err)
	}
	if got := names(t, node); got != `{"User":[{"age":41,"name":"Alice"},{"age":7,"name":"Bob"}]}` {
		t.Fatal(got)
	}
	result := node.ExecRequest(ctx, `query { Nope { name } }`)
	if len(result.GQL.Errors) == 0 {
		t.Fatal("an unknown collection is an error")
	}
}

func TestTransactionsAndPurgeByDocIDs(t *testing.T) {
	node := open(t, Options{})
	ctx := context.Background()
	saved := docs(t, collectionOf(t, node), `{"name": "Alice", "age": 40}`, `{"name": "Bob", "age": 7}`)
	if err := collectionOf(t, node).SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}

	txn, err := node.NewTxn(false)
	if err != nil {
		t.Fatal(err)
	}
	if err := collectionOf(t, txn).PurgeByDocIDs(ctx, []client.DocID{saved[0].ID()}, true); err != nil {
		t.Fatal(err)
	}
	if got := names(t, node); !strings.Contains(got, "Alice") {
		t.Fatalf("the purge lands only on commit: %s", got)
	}
	if err := txn.Commit(); err != nil {
		t.Fatal(err)
	}
	if got := names(t, node); got != `{"User":[{"age":7,"name":"Bob"}]}` {
		t.Fatal(got)
	}

	discarded, err := node.NewTxn(false)
	if err != nil {
		t.Fatal(err)
	}
	query(t, discarded, `mutation { add_User(input: {name: "Carol", age: 1}) { _docID } }`)
	discarded.Discard()
	if got := names(t, node); strings.Contains(got, "Carol") {
		t.Fatalf("a discarded transaction stores nothing: %s", got)
	}
}

func TestExportAndImportDocKVs(t *testing.T) {
	source, target := open(t, Options{}), open(t, Options{})
	ctx := context.Background()
	saved := docs(t, collectionOf(t, source), `{"name": "Alice", "age": 40}`, `{"name": "Bob", "age": 7}`)
	if err := collectionOf(t, source).SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}
	var snapshot bytes.Buffer
	frames, err := source.ExportDocKVs(ctx, "User", nil, &snapshot, false)
	if err != nil {
		t.Fatal(err)
	}
	mapping, err := source.ExportFieldMapping(ctx, "User")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(mapping), `"collectionName":"User"`) {
		t.Fatal(string(mapping))
	}
	imported, err := target.ImportRawKVsWithMapping(ctx, &snapshot, mapping)
	if err != nil {
		t.Fatal(err)
	}
	if imported != frames {
		t.Fatalf("imported %d of %d frames", imported, frames)
	}
	if err := target.RebuildCollectionIndexes(ctx, "User"); err != nil {
		t.Fatal(err)
	}
	ids := `query { User(order: {name: ASC}) { _docID } }`
	if a, b := query(t, source, ids), query(t, target, ids); a != b {
		t.Fatalf("%s != %s", a, b)
	}
	if got := query(t, target, `query { User(filter: {name: {_eq: "Bob"}}) { age } }`); got != `{"User":[{"age":7}]}` {
		t.Fatal(got)
	}
}

func TestBatchSigning(t *testing.T) {
	node := open(t, Options{EnableSigning: true})
	collector, err := node.NewBatchCIDCollector(BatchFull)
	if err != nil {
		t.Fatal(err)
	}
	ctx := ContextWithBatchSigning(context.Background(), collector)
	saved := docs(t, collectionOf(t, node), `{"name": "Alice", "age": 40}`)
	if err := collectionOf(t, node).SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}
	signature, err := SignBatch(ctx, collector, MerkleDomainSeparated)
	if err != nil {
		t.Fatal(err)
	}
	if signature.MerkleScheme != "DomainSeparated" || signature.CIDCount < 2 {
		t.Fatalf("%+v", signature)
	}
	if _, err := node.NewBatchCIDCollector("Nope"); err == nil {
		t.Fatal("an unknown scope is refused")
	}
}

func TestOpenWithRetentionFloors(t *testing.T) {
	_, err := Open(context.Background(), Options{
		RetentionFloors: map[string]RetentionFloor{"col": {Field: "blockNumber", Floor: 100}},
	})
	if err != nil {
		t.Fatal(err)
	}
}

// refuseName refuses a document whose name field is the one it names.
type refuseName struct {
	name  string
	calls atomic.Int64
}

func (f *refuseName) AllowReplication(ctx context.Context, collectionID, docID string, fields map[string]any) bool {
	f.calls.Add(1)
	return fields["name"] != f.name
}

func TestReplicatorWithAFilterRefusingOneDocument(t *testing.T) {
	ctx := context.Background()
	filter := &refuseName{name: "Mallory"}
	source := open(t, Options{ListenAddress: "/ip4/127.0.0.1/tcp/0"})
	target := open(t, Options{ListenAddress: "/ip4/127.0.0.1/tcp/0", ReplicationFilter: filter})
	addresses, err := target.PeerInfo(ctx)
	if err != nil || len(addresses) == 0 {
		t.Fatalf("peer info %v %v", addresses, err)
	}
	if err := source.AddReplicator(ctx, addresses[:1], options.AddReplicator().SetCollectionNames([]string{"User"})); err != nil {
		t.Fatal(err)
	}
	saved := docs(t, collectionOf(t, source), `{"name": "Alice", "age": 40}`, `{"name": "Mallory", "age": 1}`, `{"name": "Bob", "age": 7}`)
	if err := collectionOf(t, source).SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}
	want := `{"User":[{"age":40,"name":"Alice"},{"age":7,"name":"Bob"}]}`
	deadline := time.Now().Add(30 * time.Second)
	for {
		got := names(t, target)
		if got == want && filter.calls.Load() >= 3 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("the target holds %s after %d filter calls; want %s", got, filter.calls.Load(), want)
		}
		time.Sleep(100 * time.Millisecond)
	}
	// Mallory was refused, not just late.
	time.Sleep(time.Second)
	if got := names(t, target); got != want {
		t.Fatal(got)
	}

}

func TestDecodeFieldsAsGoDecodesCBOR(t *testing.T) {
	fields, err := decodeFields(`{"n": 5, "m": -5, "f": 1.5, "s": "x"}`)
	if err != nil {
		t.Fatal(err)
	}
	if fields["n"] != uint64(5) || fields["m"] != int64(-5) || fields["f"] != 1.5 || fields["s"] != "x" {
		t.Fatalf("%#v", fields)
	}
}
