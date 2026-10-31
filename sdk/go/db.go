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
	r := C.create_event_subscription(ptr, a.s(`["update","p2p-no-peers"]`))
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

// Events is the database's event bus: update and p2p-no-peers events.
func (d *DB) Events() event.Bus {
	return d.events
}

// Close stops the event relay and closes the database.
func (d *DB) Close() {
	close(d.stop)
	<-d.done
	_ = C.close_subscription(d.sub)
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

// relay publishes the node's events on the Go bus until Close.
//
// ponytail: polls every 10ms while idle; a blocking FFI wait removes it.
func (d *DB) relay() {
	defer close(d.done)
	for {
		select {
		case <-d.stop:
			return
		default:
		}
		r := C.poll_subscription(d.sub)
		switch r.status {
		case 0:
			raw := C.GoString(r.value)
			C.defra_free_string(r.value)
			if msg, ok := goEvent(raw); ok {
				d.events.Publish(msg)
			}
		case 2:
			select {
			case <-d.stop:
				return
			case <-time.After(10 * time.Millisecond):
			}
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
	}
	if err := json.Unmarshal([]byte(raw), &e); err != nil {
		return event.Message{}, false
	}
	switch e.Type {
	case "update":
		id, _ := cid.Decode(e.Cid)
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
	}
	return event.Message{}, false
}
