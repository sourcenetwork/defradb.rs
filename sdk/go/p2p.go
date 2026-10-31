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

	"github.com/sourcenetwork/defradb/client/options"
)

// PeerInfo is the node's full P2P addresses, each ending in its peer ID.
func (n *Node) PeerInfo(ctx context.Context, opts ...options.Enumerable[options.PeerInfoOptions]) ([]string, error) {
	o := newOptions(opts...)
	var a cargs
	defer a.free()
	raw, err := result(C.p2p_peer_info(n.ptr, a.opt(did(o.Identity))), "peer_info")
	if err != nil {
		return nil, err
	}
	var addresses []string
	if err := json.Unmarshal([]byte(raw), &addresses); err != nil {
		return nil, fmt.Errorf("defra: peer_info: %w", err)
	}
	return addresses, nil
}

// Connect connects to the peers at addresses.
func (n *Node) Connect(ctx context.Context, addresses []string, opts ...options.Enumerable[options.ConnectOptions]) error {
	o := newOptions(opts...)
	for _, address := range addresses {
		var a cargs
		_, err := result(C.p2p_connect(n.ptr, a.opt(did(o.Identity)), a.s(address)), "connect")
		a.free()
		if err != nil {
			return err
		}
	}
	return nil
}

// AddReplicator pushes the collections the options name (every collection
// when none) to the peers at addresses.
func (n *Node) AddReplicator(
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
			_, err = result(C.p2p_add_replicator(n.ptr, a.opt(did(o.Identity)), a.s(address), collections), "add_replicator")
		}
		a.free()
		if err != nil {
			return err
		}
	}
	return nil
}
