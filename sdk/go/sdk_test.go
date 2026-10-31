package defra

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	libp2pcrypto "github.com/libp2p/go-libp2p/core/crypto"
	"github.com/libp2p/go-libp2p/core/peer"
	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
	"github.com/sourcenetwork/defradb/event"
	"github.com/sourcenetwork/defradb/node"
)

const users = "type User { name: String @index age: Int }"

// start starts a node built as Shinzo builds one, with set applied to its
// options and before to the node ahead of Start, and adds the User
// collection.
func start(t *testing.T, set func(*options.NodeOptionsBuilder), before func(*Node)) *Node {
	t.Helper()
	ctx := context.Background()
	nb := options.Node()
	nb.SetDisableAPI(true)
	if set != nil {
		set(nb)
	}
	n, err := New(ctx, nb)
	if err != nil {
		t.Fatal(err)
	}
	if before != nil {
		before(n)
	}
	if err := n.Start(ctx); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = n.Close(context.Background()) })
	if _, err := n.DB.AddCollection(ctx, users); err != nil {
		t.Fatal(err)
	}
	return n
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

func docs(t *testing.T, collection client.Collection, values ...string) []*client.Document {
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

func collectionOf(t *testing.T, s client.Store) client.Collection {
	t.Helper()
	collection, err := s.GetCollectionByName(context.Background(), "User")
	if err != nil {
		t.Fatal(err)
	}
	return collection
}

func TestExecRequestAndSaveManyDocuments(t *testing.T) {
	n := start(t, nil, nil)
	ctx := context.Background()
	collection := collectionOf(t, n.DB)
	if collection.Name() != "User" {
		t.Fatalf("name %q", collection.Name())
	}
	saved := docs(t, collection, `{"name": "Alice", "age": 40}`, `{"name": "Bob", "age": 7}`)
	if err := collection.SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}
	if got := names(t, n.DB); got != `{"User":[{"age":40,"name":"Alice"},{"age":7,"name":"Bob"}]}` {
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
	if got := names(t, n.DB); got != `{"User":[{"age":41,"name":"Alice"},{"age":7,"name":"Bob"}]}` {
		t.Fatal(got)
	}
	result := n.DB.ExecRequest(ctx, `query { Nope { name } }`)
	if len(result.GQL.Errors) == 0 {
		t.Fatal("an unknown collection is an error")
	}
}

func TestDocumentsThroughTheCollection(t *testing.T) {
	n := start(t, nil, nil)
	ctx := context.Background()
	collection := collectionOf(t, n.DB)
	alice := docs(t, collection, `{"name": "Alice", "age": 40}`)[0]
	if err := collection.AddDocument(ctx, alice); err != nil {
		t.Fatal(err)
	}
	if err := collection.AddDocument(ctx, docs(t, collection, `{"name": "Alice", "age": 40}`)[0]); err == nil {
		t.Fatal("adding a document that exists is an error")
	}
	if err := collection.AddManyDocuments(ctx, docs(t, collection, `{"name": "Bob", "age": 7}`, `{"name": "Carol", "age": 3}`)); err != nil {
		t.Fatal(err)
	}

	got, err := collection.GetDocument(ctx, alice.ID())
	if err != nil {
		t.Fatal(err)
	}
	if name, _ := got.Get("name"); name != "Alice" || got.ID() != alice.ID() {
		t.Fatalf("got %v %v", name, got.ID())
	}
	if err := got.Set(ctx, "age", 41); err != nil {
		t.Fatal(err)
	}
	if err := collection.UpdateDocument(ctx, got); err != nil {
		t.Fatal(err)
	}

	deleted, err := collection.DeleteDocumentsWithFilter(ctx, map[string]any{"age": map[string]any{"_lt": 5}})
	if err != nil {
		t.Fatal(err)
	}
	if deleted.Count != 1 {
		t.Fatalf("deleted %+v", deleted)
	}
	if got := names(t, n.DB); got != `{"User":[{"age":41,"name":"Alice"},{"age":7,"name":"Bob"}]}` {
		t.Fatal(got)
	}

	all, err := n.DB.GetCollections(ctx, options.GetCollections().SetCollectionID(collection.CollectionID()))
	if err != nil || len(all) != 1 || all[0].Name() != "User" {
		t.Fatalf("collections %v %v", all, err)
	}
	if err := collection.Truncate(ctx); err != nil {
		t.Fatal(err)
	}
	if got := names(t, n.DB); got != `{"User":[]}` {
		t.Fatal(got)
	}
	if _, err := n.DB.ListReplicators(ctx); !errors.Is(err, ErrNotSupported) {
		t.Fatalf("an unoffered method says so: %v", err)
	}
}

func TestTransactionsAndPurgeByDocIDs(t *testing.T) {
	n := start(t, nil, nil)
	ctx := context.Background()
	saved := docs(t, collectionOf(t, n.DB), `{"name": "Alice", "age": 40}`, `{"name": "Bob", "age": 7}`)
	if err := collectionOf(t, n.DB).SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}

	txn, err := n.DB.NewTxn(false)
	if err != nil {
		t.Fatal(err)
	}
	if err := collectionOf(t, txn).PurgeByDocIDs(ctx, []client.DocID{saved[0].ID()}, true); err != nil {
		t.Fatal(err)
	}
	if got := names(t, n.DB); !strings.Contains(got, "Alice") {
		t.Fatalf("the purge lands only on commit: %s", got)
	}
	if err := txn.Commit(); err != nil {
		t.Fatal(err)
	}
	if got := names(t, n.DB); got != `{"User":[{"age":7,"name":"Bob"}]}` {
		t.Fatal(got)
	}

	discarded, err := n.DB.NewTxn(false)
	if err != nil {
		t.Fatal(err)
	}
	query(t, discarded, `mutation { add_User(input: {name: "Carol", age: 1}) { _docID } }`)
	discarded.Discard()
	if got := names(t, n.DB); strings.Contains(got, "Carol") {
		t.Fatalf("a discarded transaction stores nothing: %s", got)
	}
}

func TestExportAndImportDocKVs(t *testing.T) {
	source, target := start(t, nil, nil), start(t, nil, nil)
	ctx := context.Background()
	saved := docs(t, collectionOf(t, source.DB), `{"name": "Alice", "age": 40}`, `{"name": "Bob", "age": 7}`)
	if err := collectionOf(t, source.DB).SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}
	var snapshot bytes.Buffer
	frames, err := source.DB.ExportDocKVs(ctx, "User", nil, &snapshot, false)
	if err != nil {
		t.Fatal(err)
	}
	mapping, err := source.DB.ExportFieldMapping(ctx, "User")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(mapping), `"collectionName":"User"`) {
		t.Fatal(string(mapping))
	}
	imported, err := target.DB.ImportRawKVsWithMapping(ctx, &snapshot, mapping)
	if err != nil {
		t.Fatal(err)
	}
	if imported != frames {
		t.Fatalf("imported %d of %d frames", imported, frames)
	}
	if err := target.DB.RebuildCollectionIndexes(ctx, "User"); err != nil {
		t.Fatal(err)
	}
	ids := `query { User(order: {name: ASC}) { _docID } }`
	if a, b := query(t, source.DB, ids), query(t, target.DB, ids); a != b {
		t.Fatalf("%s != %s", a, b)
	}
	if got := query(t, target.DB, `query { User(filter: {name: {_eq: "Bob"}}) { age } }`); got != `{"User":[{"age":7}]}` {
		t.Fatal(got)
	}
}

func TestGoBatchCollectorGetsEachComposite(t *testing.T) {
	n := start(t, nil, nil)
	collector := node.NewBatchCIDCollector()
	ctx := node.ContextWithBatchSigning(context.Background(), collector)
	collection := collectionOf(t, n.DB)
	if err := collection.AddManyDocuments(ctx, docs(t, collection, `{"name": "Alice", "age": 40}`, `{"name": "Bob", "age": 7}`)); err != nil {
		t.Fatal(err)
	}
	query(t, n.DB, `mutation { add_User(input: {name: "Carol", age: 1}) { _docID } }`)
	if collector.Len() != 2 {
		t.Fatalf("one composite per document written with the collector: %v", collector.GetCIDs())
	}
	if len(node.ComputeMerkleRoot(collector.GetCIDs())) == 0 {
		t.Fatal("no merkle root")
	}
}

func TestEventsCarryEachUpdate(t *testing.T) {
	n := start(t, nil, nil)
	sub, err := n.DB.Events().Subscribe(event.UpdateName)
	if err != nil {
		t.Fatal(err)
	}
	collection := collectionOf(t, n.DB)
	alice := docs(t, collection, `{"name": "Alice", "age": 40}`)[0]
	if err := collection.AddDocument(context.Background(), alice); err != nil {
		t.Fatal(err)
	}
	select {
	case msg := <-sub.Message():
		update, ok := msg.Data.(event.Update)
		if !ok || update.DocID != alice.ID().String() || update.CollectionID != collection.CollectionID() ||
			update.IsRelay || !update.Cid.Defined() || len(update.Block) == 0 {
			t.Fatalf("%+v", msg)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("no update event")
	}
}

func TestStartServesGoDefraDBsHTTPAPI(t *testing.T) {
	n := start(t, func(nb *options.NodeOptionsBuilder) {
		nb.SetDisableAPI(false)
		nb.HTTP().SetAddress("127.0.0.1:0")
	}, nil)
	if n.APIURL == "" {
		t.Fatal("no API URL")
	}
	query(t, n.DB, `mutation { add_User(input: {name: "Alice", age: 40}) { _docID } }`)
	body := strings.NewReader(`{"query": "query { User { name } }"}`)
	resp, err := http.Post(n.APIURL+"/api/v0/graphql", "application/json", body)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	raw, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK || !strings.Contains(string(raw), "Alice") {
		t.Fatalf("%d %s", resp.StatusCode, raw)
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

// ageFloor is a retention rule on every collection's age field, asked as
// documents arrive, as Shinzo's host resolves its collections only once the
// node has started.
type ageFloor struct {
	floor int64
	calls atomic.Int64
}

func (r *ageFloor) RetentionFloor(collectionID string) (string, int64, bool) {
	r.calls.Add(1)
	return "age", r.floor, true
}

func TestReplicationThroughTheFilterAndTheRetentionRule(t *testing.T) {
	ctx := context.Background()
	filter := &refuseName{name: "Mallory"}
	rule := &ageFloor{floor: 3}
	listen := func(nb *options.NodeOptionsBuilder) { nb.P2P().SetListenAddresses("/ip4/127.0.0.1/tcp/0") }
	source := start(t, listen, nil)
	target := start(t, listen, func(n *Node) {
		n.ReplicationFilter = filter
		n.RetentionRule = rule
	})
	merges, err := target.DB.Events().Subscribe(event.MergeCompleteName)
	if err != nil {
		t.Fatal(err)
	}
	addresses, err := target.DB.PeerInfo(ctx)
	if err != nil || len(addresses) == 0 {
		t.Fatalf("peer info %v %v", addresses, err)
	}
	if err := source.DB.AddReplicator(ctx, addresses[:1], options.AddReplicator().SetCollectionNames([]string{"User"})); err != nil {
		t.Fatal(err)
	}
	saved := docs(t, collectionOf(t, source.DB),
		`{"name": "Alice", "age": 40}`, `{"name": "Mallory", "age": 9}`, `{"name": "Bob", "age": 7}`, `{"name": "Tiny", "age": 2}`)
	if err := collectionOf(t, source.DB).SaveManyDocuments(ctx, saved); err != nil {
		t.Fatal(err)
	}
	want := `{"User":[{"age":40,"name":"Alice"},{"age":7,"name":"Bob"}]}`
	deadline := time.Now().Add(30 * time.Second)
	for {
		got := names(t, target.DB)
		if got == want && filter.calls.Load() >= 1 && rule.calls.Load() >= 1 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("the target holds %s after %d filter and %d rule calls; want %s",
				got, filter.calls.Load(), rule.calls.Load(), want)
		}
		time.Sleep(100 * time.Millisecond)
	}
	// Mallory and Tiny were refused, not just late.
	time.Sleep(time.Second)
	if got := names(t, target.DB); got != want {
		t.Fatal(got)
	}
	select {
	case msg := <-merges.Message():
		if merge, ok := msg.Data.(event.MergeComplete); !ok || merge.Merge.DocID == "" || !merge.Merge.Cid.Defined() {
			t.Fatalf("%+v", msg)
		}
	default:
		t.Fatal("no merge-complete event for the replicated documents")
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

func TestSetPrivateKeyGivesGosPeerID(t *testing.T) {
	key, _, err := libp2pcrypto.GenerateEd25519Key(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	raw, err := key.Raw()
	if err != nil {
		t.Fatal(err)
	}
	want, err := peer.IDFromPrivateKey(key)
	if err != nil {
		t.Fatal(err)
	}
	n := start(t, func(nb *options.NodeOptionsBuilder) {
		nb.P2P().SetListenAddresses("/ip4/127.0.0.1/tcp/0").SetPrivateKey(raw)
	}, nil)
	addresses, err := n.DB.PeerInfo(context.Background())
	if err != nil || len(addresses) == 0 {
		t.Fatalf("peer info %v %v", addresses, err)
	}
	if !strings.HasSuffix(addresses[0], "/p2p/"+want.String()) {
		t.Fatalf("%s is not peer %s", addresses[0], want)
	}
}

func TestEveryListenAddressAndTheResourceLimits(t *testing.T) {
	n := start(t, func(nb *options.NodeOptionsBuilder) {
		nb.P2P().
			SetListenAddresses("/ip4/127.0.0.1/tcp/0", "/ip4/127.0.0.2/tcp/0").
			SetResourceMemoryMiB(256).
			SetResourceFileDescriptors(512).
			SetMaxStreamsPerPeer(64).
			SetEnablePubSub(true)
	}, nil)
	addresses, err := n.DB.PeerInfo(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	joined := strings.Join(addresses, " ")
	if !strings.Contains(joined, "/ip4/127.0.0.1/") || !strings.Contains(joined, "/ip4/127.0.0.2/") {
		t.Fatalf("listening on %v", addresses)
	}
}

func TestTheEventRelayParksWhileIdle(t *testing.T) {
	n := start(t, nil, nil)
	sub, err := n.DB.Events().Subscribe(event.UpdateName)
	if err != nil {
		t.Fatal(err)
	}
	before := n.DB.waits.Load()
	time.Sleep(1500 * time.Millisecond)
	// A wait lasts up to a second, so an idle relay makes about two.
	if idle := n.DB.waits.Load() - before; idle > 3 {
		t.Fatalf("the idle relay waited %d times in 1.5s", idle)
	}
	query(t, n.DB, `mutation { add_User(input: {name: "Alice", age: 40}) { _docID } }`)
	select {
	case <-sub.Message():
	case <-time.After(5 * time.Second):
		t.Fatal("an event arrives while the relay is parked")
	}
}
