package defra

/*
#include <stdlib.h>
#include "defra.h"
extern int defraSDKReplicationFilterShim(uintptr_t context, const char *collection_id,
                                         const char *doc_id, const char *fields_json);
extern int defraSDKRetentionRuleShim(uintptr_t context, const char *collection_id,
                                     char *field_out, size_t field_capacity, int64_t *floor_out);
*/
import "C"

import (
	"context"
	"errors"
	gohttp "net/http"
	"runtime/cgo"
	"unsafe"

	"github.com/sourcenetwork/defradb/acp/identity"
	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
	defrahttp "github.com/sourcenetwork/defradb/http"
	"github.com/sourcenetwork/defradb/node"
)

// Node is a defradb.rs node in this process, shaped as Go DefraDB's
// `node.Node`: build it with New, set ReplicationFilter and RetentionRule,
// then Start it. DB is set once it has started.
type Node struct {
	// DB is the database; nil until Start.
	DB *DB
	// APIURL is where Go DefraDB's HTTP API serves DB, once started, unless
	// the API is disabled.
	APIURL string
	// ReplicationFilter refuses replicated documents before anything of them
	// is stored. Set it between New and Start.
	ReplicationFilter client.ReplicationFilter
	// RetentionRule refuses replicated documents by height. Set it between
	// New and Start.
	RetentionRule client.RetentionRule

	opts    *options.NodeOptions
	handles []cgo.Handle
	server  *defrahttp.Server
	handler *defrahttp.Handler
}

// New resolves opts over Go DefraDB's defaults; nothing opens until Start.
func New(ctx context.Context, opts ...options.Enumerable[options.NodeOptions]) (*Node, error) {
	resolved := node.DefaultNodeOptions()
	applyOptions(&resolved, opts...)
	return &Node{opts: &resolved}, nil
}

// Options are the node's resolved options.
func (n *Node) Options() *options.NodeOptions {
	return n.opts
}

// Start opens the database, its P2P system when P2P is enabled and given a
// listen address, and Go DefraDB's HTTP API over it unless the API is
// disabled.
//
// Of the options it honours the store path and in-memory store, DisableP2P,
// DisableAPI, the HTTP options, the DB identity (which signs the blocks the
// node writes when signing is on) and every P2P option but
// EnableClearBackoffOnRetry. BadgerFileSize, BadgerEncryptionKey and
// LensRuntime have no Rust equivalent and are ignored.
func (n *Node) Start(ctx context.Context) error {
	initFFI()
	o := n.opts
	var a cargs
	defer a.free()

	var c C.struct_NodeInitOptions
	if o.Store.Path != "" && !o.Store.BadgerInMemory && o.Store.Store != options.NodeMemoryStore {
		c.db_path = a.s(o.Store.Path)
	} else {
		c.in_memory = 1
	}
	c.iroh_discovery = 1
	c.shinzo_wire = 1
	if o.DB.Identity.HasValue() {
		full, ok := o.DB.Identity.Value().(identity.FullIdentity)
		if !ok {
			return errors.New("defra: the node identity has no private key")
		}
		key := full.PrivateKey()
		raw := key.Raw()
		c.signing_key_type = a.s(string(key.Type()))
		c.signing_private_key = (*C.uint8_t)(C.CBytes(raw))
		a.p = append(a.p, unsafe.Pointer(c.signing_private_key))
		c.signing_private_key_len = C.size_t(len(raw))
		if o.DB.EnableSigning {
			c.enable_signing = 1
		}
	}
	if n.ReplicationFilter != nil {
		h := cgo.NewHandle(n.ReplicationFilter)
		n.handles = append(n.handles, h)
		c.replication_filter = (*[0]byte)(C.defraSDKReplicationFilterShim)
		c.replication_filter_context = C.uintptr_t(h)
	}
	if n.RetentionRule != nil {
		h := cgo.NewHandle(n.RetentionRule)
		n.handles = append(n.handles, h)
		c.retention_rule = (*[0]byte)(C.defraSDKRetentionRuleShim)
		c.retention_rule_context = C.uintptr_t(h)
	}

	var r C.struct_NewNodeResult
	if !o.DisableP2P && len(o.P2P.ListenAddresses) > 0 {
		if err := p2pOptions(&c, &a, o.P2P); err != nil {
			return err
		}
		r = C.new_node_with_p2p(c, nil)
	} else {
		r = C.new_node(c)
	}
	if r.status != 0 {
		message := C.GoString(r.error)
		C.defra_free_string(r.error)
		n.freeHandles()
		return ffiError("start", message)
	}
	db, err := openDB(r.node_ptr)
	if err != nil {
		_, _ = result(C.node_close(r.node_ptr), "close")
		n.freeHandles()
		return err
	}
	n.DB = db
	if o.DisableAPI {
		return nil
	}
	return n.startAPI()
}

// startAPI serves Go DefraDB's HTTP handler over DB.
func (n *Node) startAPI() error {
	handler, err := defrahttp.NewHandler(n.DB, n.opts)
	if err != nil {
		return err
	}
	server, err := defrahttp.NewServer(handler, options.NodeHTTP().SetAll(n.opts.HTTP))
	if err != nil {
		handler.Close()
		return err
	}
	if err := server.SetListener(); err != nil {
		handler.Close()
		return err
	}
	go func() {
		if err := server.Serve(); err != nil && !errors.Is(err, gohttp.ErrServerClosed) {
			return
		}
	}()
	n.handler, n.server, n.APIURL = handler, server, server.Address()
	return nil
}

// Close stops the API and closes the database.
func (n *Node) Close(ctx context.Context) error {
	var err error
	if n.server != nil {
		err = n.server.Shutdown(ctx)
	}
	if n.handler != nil {
		n.handler.Close()
	}
	if n.DB != nil {
		n.DB.Close()
	}
	n.freeHandles()
	return err
}

func (n *Node) freeHandles() {
	for _, h := range n.handles {
		h.Delete()
	}
	n.handles = nil
}

// p2pOptions puts Go DefraDB's P2P options into c: every listen address and
// bootstrap peer, GossipSub and the relay as Go switches them (off unless
// set), the peer key and the resource limits.
func p2pOptions(c *C.struct_NodeInitOptions, a *cargs, o options.NodeP2POptions) error {
	listen, err := a.json(o.ListenAddresses)
	if err != nil {
		return err
	}
	c.p2p_listen_addrs_json = listen
	if len(o.BootstrapPeers) > 0 {
		if c.p2p_bootstrap_peers_json, err = a.json(o.BootstrapPeers); err != nil {
			return err
		}
	}
	if !o.EnablePubSub {
		c.p2p_disable_pubsub = 1
	}
	if !o.EnableRelay {
		c.p2p_disable_relay = 1
	}
	if len(o.PrivateKey) > 0 {
		c.p2p_private_key = (*C.uint8_t)(C.CBytes(o.PrivateKey))
		a.p = append(a.p, unsafe.Pointer(c.p2p_private_key))
		c.p2p_private_key_len = C.size_t(len(o.PrivateKey))
	}
	if o.ResourceMemoryMiB > 0 {
		c.p2p_resource_memory_mib = C.uint64_t(o.ResourceMemoryMiB)
	}
	if o.ResourceFileDescriptors > 0 {
		c.p2p_resource_file_descriptors = C.uint64_t(o.ResourceFileDescriptors)
	}
	if o.MaxStreamsPerPeer > 0 {
		c.p2p_max_streams_per_peer = C.size_t(o.MaxStreamsPerPeer)
	}
	return nil
}
