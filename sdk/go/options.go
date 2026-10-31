package defra

import (
	"github.com/sourcenetwork/defradb/acp/identity"
	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/immutable"
	"github.com/sourcenetwork/immutable/enumerable"
)

// Options configures a node: Go Shinzo's `node.Node` fields and the store and
// network options of its `options.Node()` builder that Shinzo sets.
type Options struct {
	// Path is the data directory; empty keeps the node in memory.
	Path string
	// ListenAddress opens the node with P2P on this multiaddr (for example
	// /ip4/127.0.0.1/tcp/0); empty opens it without P2P.
	ListenAddress string
	// EnableSigning signs the blocks the node writes with its identity.
	EnableSigning bool
	// ShinzoWire speaks Go Shinzo's P2P wire.
	ShinzoWire bool
	// RetentionFloors refuses replicated documents by block height, keyed by
	// CollectionID: Go's `client.RetentionRule`, given as the floors it
	// returns. A static map, as defradb.rs's rule is.
	RetentionFloors map[string]RetentionFloor
	// ReplicationFilter is asked whether to keep each replicated document
	// before anything of it is stored. It is called from many threads at
	// once and must not block.
	ReplicationFilter client.ReplicationFilter
}

// RetentionFloor is one collection's retention rule: the integer field that
// holds a document's height, and the floor at or below which a replicated
// document is refused.
type RetentionFloor struct {
	Field string `json:"field"`
	Floor int64  `json:"floor"`
}

// newOptions applies Go DefraDB's functional options to a zero T.
func newOptions[T any](opts ...enumerable.Enumerable[func(*T)]) *T {
	o := new(T)
	for _, e := range opts {
		if e == nil {
			continue
		}
		e.Reset()
		for {
			ok, err := e.Next()
			if err != nil || !ok {
				break
			}
			apply, err := e.Value()
			if err == nil && apply != nil {
				apply(o)
			}
		}
	}
	return o
}

// did is the DID an option's identity names, or empty for none.
func did(id immutable.Option[identity.Identity]) string {
	if !id.HasValue() || id.Value() == nil {
		return ""
	}
	return id.Value().DID()
}
