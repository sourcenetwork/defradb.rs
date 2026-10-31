package defra

/*
#include <stdlib.h>
#include "defra.h"
*/
import "C"

import (
	"context"
	"fmt"

	"github.com/sourcenetwork/defradb/client/options"
)

// PeerInfo is the node's full P2P addresses, each ending in its peer ID.
func (s *store) PeerInfo(ctx context.Context, opts ...options.Enumerable[options.PeerInfoOptions]) ([]string, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	raw, err := result(C.p2p_peer_info(s.ptr, a.opt(did(o.Identity))), "peer_info")
	if err != nil {
		return nil, err
	}
	return decode[[]string](raw, "peer_info")
}

// ActivePeers is the peers the node is connected to.
func (s *store) ActivePeers(ctx context.Context, opts ...options.Enumerable[options.ActivePeersOptions]) ([]string, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	raw, err := result(C.p2p_active_peers(s.ptr, a.opt(did(o.Identity))), "active_peers")
	if err != nil {
		return nil, err
	}
	return decode[[]string](raw, "active_peers")
}

// Connect connects to the peers at addresses.
func (s *store) Connect(ctx context.Context, addresses []string, opts ...options.Enumerable[options.ConnectOptions]) error {
	o := newOptions(opts...)
	for _, address := range addresses {
		var a cargs
		_, err := result(C.p2p_connect(s.ptr, a.opt(did(o.Identity)), a.s(address)), "connect")
		a.free()
		if err != nil {
			return err
		}
	}
	return nil
}

// AddReplicator pushes the collections the options name (every collection
// when none) to the peers at addresses.
func (s *store) AddReplicator(
	ctx context.Context,
	addresses []string,
	opts ...options.Enumerable[options.AddReplicatorOptions],
) error {
	if len(addresses) == 0 {
		return fmt.Errorf("defra: add_replicator: at least one address is required")
	}
	o := newOptions(opts...)
	for _, address := range addresses {
		var a cargs
		collections, err := a.json(o.CollectionNames)
		if err == nil {
			_, err = result(C.p2p_add_replicator(s.ptr, a.opt(did(o.Identity)), a.s(address), collections), "add_replicator")
		}
		a.free()
		if err != nil {
			return err
		}
	}
	return nil
}

// AddP2PCollections subscribes the node to the named collections' topics.
func (s *store) AddP2PCollections(
	ctx context.Context,
	collectionNames []string,
	opts ...options.Enumerable[options.AddP2PCollectionsOptions],
) error {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	names, err := a.json(collectionNames)
	if err != nil {
		return err
	}
	_, err = result(C.p2p_add_collections(s.ptr, a.opt(did(o.Identity)), names), "add_p2p_collections")
	return err
}

// DeleteP2PCollections unsubscribes the node from the named collections'
// topics.
func (s *store) DeleteP2PCollections(
	ctx context.Context,
	collectionNames []string,
	opts ...options.Enumerable[options.DeleteP2PCollectionsOptions],
) error {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	names, err := a.json(collectionNames)
	if err != nil {
		return err
	}
	_, err = result(C.p2p_delete_collections(s.ptr, a.opt(did(o.Identity)), names), "delete_p2p_collections")
	return err
}
