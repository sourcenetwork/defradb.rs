package defra

/*
#include <stdlib.h>
#include "defra.h"
*/
import "C"

import (
	"encoding/base64"
	"encoding/json"
	"strconv"
	"sync/atomic"
	"time"

	"github.com/ipfs/go-cid"
	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/event"
)

// DB is a started node's database: Go DefraDB's `client.TxnStore` and
// `client.P2P`, with its event bus, so Go DefraDB's HTTP handler serves it.
// It is safe for concurrent use.
type DB struct {
	store
	events event.Bus
	sub    C.uintptr_t
	stop   chan struct{}
	done   chan struct{}
	waits  atomic.Int64 // library waits the relay made, for its idle test
}

var (
	_ client.TxnStore = (*DB)(nil)
	_ client.Txn      = (*Txn)(nil)
)

// openDB is the database the node handle ptr names, its events relayed to
// a Go bus.
func openDB(ptr C.uintptr_t) (*DB, error) {
	var a cargs
	defer a.free()
	r := C.create_event_subscription(ptr, a.s(relayedEvents))
	if r.status != 0 {
		message := C.GoString(r.error)
		C.defra_free_string(r.error)
		return nil, ffiError("events", message)
	}
	d := &DB{
		events: event.NewChannelBus(0, 100),
		sub:    r.subscription_handle,
		stop:   make(chan struct{}),
		done:   make(chan struct{}),
	}
	d.store = store{ptr: ptr}
	go d.relay()
	return d, nil
}

// relayedEvents are the events of Go's bus the Rust node emits with the data
// Go's event types carry.
const relayedEvents = `["update","p2p-no-peers","merge-complete","topic-peer-event","se-artifact-received"]`

// Events is the database's event bus: update, p2p-no-peers, merge-complete,
// topic-peer-event and se-artifact-received events.
func (d *DB) Events() event.Bus {
	return d.events
}

// Close stops the event relay and closes the database.
func (d *DB) Close() {
	close(d.stop)
	_ = C.close_subscription(d.sub)
	<-d.done
	_, _ = result(C.node_close(d.ptr), "close")
	d.events.Close()
}

// NewTxn begins a transaction; its writes land when it commits.
func (d *DB) NewTxn(readOnly bool) (client.Txn, error) {
	readonly := C.int32_t(0)
	if readOnly {
		readonly = 1
	}
	r := C.begin_txn(d.ptr, readonly)
	if r.status != 0 {
		message := C.GoString(r.error)
		C.defra_free_string(r.error)
		return nil, ffiError("new_txn", message)
	}
	id := C.GoString(r.txn_id)
	C.defra_free_string(r.txn_id)
	return &Txn{store: store{ptr: d.ptr, txn: id}, started: time.Now()}, nil
}

// Txn is a transaction: Go's `client.Txn`, a store whose writes land when
// it commits.
type Txn struct {
	store
	started time.Time
}

// ID is the transaction's identifier.
func (t *Txn) ID() uint64 {
	id, _ := strconv.ParseUint(t.txn, 10, 64)
	return id
}

// StartTS is when the transaction began.
func (t *Txn) StartTS() time.Time {
	return t.started
}

// Commit commits the transaction.
func (t *Txn) Commit() error {
	var a cargs
	defer a.free()
	_, err := result(C.commit_txn(t.ptr, a.s(t.txn)), "commit")
	return err
}

// Discard discards the transaction: nothing it wrote lands.
func (t *Txn) Discard() {
	var a cargs
	defer a.free()
	_, _ = result(C.rollback_txn(t.ptr, a.s(t.txn)), "discard")
}

// relay publishes the node's events on the Go bus until Close, parked in
// the library's wait between events: Close's close_subscription wakes it.
func (d *DB) relay() {
	defer close(d.done)
	for {
		select {
		case <-d.stop:
			return
		default:
		}
		d.waits.Add(1)
		r := C.wait_subscription(d.sub, 1000)
		switch r.status {
		case 0:
			raw := C.GoString(r.value)
			C.defra_free_string(r.value)
			if msg, ok := goEvent(raw); ok {
				d.events.Publish(msg)
			}
		case 2:
		default:
			if r.error != nil {
				C.defra_free_string(r.error)
			}
			return
		}
	}
}

// goEvent is the Go DefraDB event the FFI's event JSON describes.
func goEvent(raw string) (event.Message, bool) {
	var e struct {
		Type         string `json:"type"`
		DocID        string `json:"doc_id"`
		Cid          string `json:"cid"`
		CollectionID string `json:"collection_id"`
		Block        string `json:"block"`
		IsRetry      bool   `json:"is_retry"`
		IsRelay      bool   `json:"is_relay"`
		Topic        string `json:"topic"`
		ByPeer       string `json:"by_peer"`
		PeerID       string `json:"peer_id"`
		EventType    string `json:"event_type"`
	}
	if err := json.Unmarshal([]byte(raw), &e); err != nil {
		return event.Message{}, false
	}
	id, _ := cid.Decode(e.Cid)
	switch e.Type {
	case "update":
		block, _ := base64.StdEncoding.DecodeString(e.Block)
		return event.NewMessage(event.UpdateName, event.Update{
			DocID:        e.DocID,
			Cid:          id,
			CollectionID: e.CollectionID,
			Block:        block,
			IsRetry:      e.IsRetry,
			IsRelay:      e.IsRelay,
		}), true
	case "p2p_no_peers":
		return event.NewMessage(event.P2PNoPeersName, event.P2PNoPeers{
			DocID:        e.DocID,
			CollectionID: e.CollectionID,
			Topic:        e.Topic,
		}), true
	case "merge_complete":
		return event.NewMessage(event.MergeCompleteName, event.MergeComplete{Merge: event.Merge{
			DocID:        e.DocID,
			ByPeer:       e.ByPeer,
			Cid:          id,
			CollectionID: e.CollectionID,
		}}), true
	case "topic_peer_event":
		return event.NewMessage(event.TopicPeerEventName, event.TopicPeerEvent{
			PeerID:    e.PeerID,
			Topic:     e.Topic,
			EventType: e.EventType,
		}), true
	case "se_artifact_received":
		return event.NewMessage(event.SEArtifactReceivedName, event.SEArtifactReceived{DocID: e.DocID}), true
	}
	return event.Message{}, false
}
