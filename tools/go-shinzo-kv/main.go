// Command kvsnap runs Go Shinzo's KV snapshot export against a stopped
// node's root directory. Go reaches it only in-process: its CLI and HTTP
// client refuse it with ErrNotSupportedViaHTTP. fixtures.sh builds this file
// inside a shinzo-develop checkout (`go build ./cmd/kvsnap`) to write the
// snapshots defradb.rs's import tests read.
//
//	kvsnap export <rootdir> <collection> <out> [docID ...]
//	kvsnap export-datastore <rootdir> <collection> <out> [docID ...]
//	kvsnap add [--encrypt] <rootdir> <sdl> <collection> <doc JSON> ...
//
// Each export writes <out> and its field mapping, <out>.mapping.json. add
// makes the collections in <sdl> and adds the documents to <collection> as
// a fresh Ed25519 identity, signed (and with --encrypt, encrypted), printing
// each DocID: Go signs a block only when the request's identity holds its
// private key, which only an in-process caller can give it.
package main

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/sourcenetwork/defradb/acp/identity"
	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
	"github.com/sourcenetwork/defradb/crypto"
	"github.com/sourcenetwork/defradb/node"
)

func main() {
	if err := run(context.Background(), os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func open(ctx context.Context, rootdir string) (*node.Node, error) {
	opts := options.Node().
		SetDisableP2P(true).
		SetDisableAPI(true).
		SetEnableDevelopment(true)
	opts.Store().SetPath(filepath.Join(rootdir, "data"))
	opts.DocumentACP().SetPath(rootdir).NodeACP().SetPath(rootdir)
	n, err := node.New(ctx, opts)
	if err != nil {
		return nil, err
	}
	return n, n.Start(ctx)
}

func run(ctx context.Context, args []string) error {
	if len(args) > 0 && args[0] == "add" {
		return add(ctx, args[1:])
	}
	if len(args) < 4 || (args[0] != "export" && args[0] != "export-datastore") {
		return fmt.Errorf("usage: kvsnap export|export-datastore <rootdir> <collection> <out> [docID ...] | add [--encrypt] <rootdir> <sdl> <collection> <doc JSON> ...")
	}
	n, err := open(ctx, args[1])
	if err != nil {
		return err
	}
	defer n.DB.Close()
	f, err := os.Create(args[3])
	if err != nil {
		return err
	}
	defer func() { _ = f.Close() }()
	count, err := n.DB.ExportDocKVs(ctx, args[2], args[4:], f, args[0] == "export-datastore")
	if err != nil {
		return err
	}
	mapping, err := n.DB.ExportFieldMapping(ctx, args[2])
	if err != nil {
		return err
	}
	if err := os.WriteFile(args[3]+".mapping.json", mapping, 0o600); err != nil {
		return err
	}
	fmt.Println(count)
	return nil
}

func add(ctx context.Context, args []string) error {
	encrypt := len(args) > 0 && args[0] == "--encrypt"
	if encrypt {
		args = args[1:]
	}
	if len(args) < 4 {
		return fmt.Errorf("usage: kvsnap add [--encrypt] <rootdir> <sdl> <collection> <doc JSON> ...")
	}
	n, err := open(ctx, args[0])
	if err != nil {
		return err
	}
	defer n.DB.Close()
	if _, err := n.DB.AddCollection(ctx, args[1]); err != nil {
		return err
	}
	col, err := n.DB.GetCollectionByName(ctx, args[2])
	if err != nil {
		return err
	}
	signer, err := identity.Generate(crypto.KeyTypeEd25519)
	if err != nil {
		return err
	}
	for _, raw := range args[3:] {
		doc, err := client.NewDocFromJSON(ctx, []byte(raw), col.Version())
		if err != nil {
			return err
		}
		opts := options.AddDocument().
			SetIdentity(signer).
			SetEnableSigning(true).
			SetEncryptDoc(encrypt)
		if err := col.AddDocument(ctx, doc, opts); err != nil {
			return err
		}
		fmt.Println(doc.ID())
	}
	return nil
}
