package defra

/*
#include <stdlib.h>
#include "defra.h"
*/
import "C"

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
)

// MerkleScheme is how a batch signature's Merkle tree is built.
type MerkleScheme string

const (
	// MerkleGoShinzo is Go Shinzo's tree, which its nodes verify.
	MerkleGoShinzo MerkleScheme = "GoShinzo"
	// MerkleDomainSeparated tags leaves and inner nodes, so no two CID lists
	// share a root. Go Shinzo does not verify it yet.
	MerkleDomainSeparated MerkleScheme = "DomainSeparated"
)

// BatchScope is which written blocks a batch signature covers.
type BatchScope string

const (
	// BatchComposite is each document's composite block, as Go Shinzo
	// collects them.
	BatchComposite BatchScope = "Composite"
	// BatchFull is every block a write produces.
	BatchFull BatchScope = "Full"
)

// BatchCIDCollector collects the blocks the writes made with it on their
// context write, for one batch signature (Go Shinzo's `node.BatchCIDCollector`).
type BatchCIDCollector struct {
	node    *Node
	session string
}

// BatchSignature is a batch's signature over the Merkle root of its CIDs, as
// Go Shinzo's `node.BatchSignature` serializes it.
type BatchSignature struct {
	SigType      string `json:"sig_type"`
	Identity     string `json:"identity"`
	Value        string `json:"value"`
	MerkleRoot   string `json:"merkle_root"`
	CIDCount     int    `json:"cid_count"`
	MerkleScheme string `json:"merkle_scheme,omitempty"`
	BatchScope   string `json:"batch_scope,omitempty"`
}

type batchKey struct{}

// NewBatchCIDCollector starts a batch on the node, collecting the blocks
// scope names; empty is Go Shinzo's composite scope.
func (n *Node) NewBatchCIDCollector(scope BatchScope) (*BatchCIDCollector, error) {
	id := make([]byte, 16)
	if _, err := rand.Read(id); err != nil {
		return nil, err
	}
	collector := &BatchCIDCollector{node: n, session: hex.EncodeToString(id)}
	var a cargs
	defer a.free()
	_, err := result(C.batch_start_with_options(n.ptr, nil, a.s(collector.session), a.opt(string(scope))), "batch_start")
	if err != nil {
		return nil, err
	}
	return collector, nil
}

// ContextWithBatchSigning is ctx with collector: the writes made with it
// are collected into the batch.
func ContextWithBatchSigning(ctx context.Context, collector *BatchCIDCollector) context.Context {
	return context.WithValue(ctx, batchKey{}, collector)
}

// batchSession is the batch session ctx carries, or empty.
func batchSession(ctx context.Context) string {
	if ctx == nil {
		return ""
	}
	if collector, ok := ctx.Value(batchKey{}).(*BatchCIDCollector); ok && collector != nil {
		return collector.session
	}
	return ""
}

// SignBatch signs the collected CIDs' Merkle root, built as scheme says;
// empty is Go Shinzo's tree. The batch ends.
func SignBatch(ctx context.Context, collector *BatchCIDCollector, scheme MerkleScheme) (*BatchSignature, error) {
	var a cargs
	defer a.free()
	raw, err := result(C.batch_sign_with_options(
		collector.node.ptr, nil, a.s(collector.session), a.opt(string(scheme)),
	), "sign_batch")
	if err != nil {
		return nil, err
	}
	var signature BatchSignature
	if err := json.Unmarshal([]byte(raw), &signature); err != nil {
		return nil, fmt.Errorf("defra: sign_batch: %w", err)
	}
	return &signature, nil
}
