// Command kvsnap runs Go Shinzo's KV snapshot export and import against a
// stopped node's root directory. Go reaches them only in-process: its CLI and
// HTTP client refuse them with ErrNotSupportedViaHTTP. CI builds this file
// inside the pinned shinzo-develop checkout (`go build ./cmd/kvsnap`).
//
//	kvsnap export <rootdir> <collection> <out> [docID ...]
//	kvsnap import <rootdir> <in> <mapping|-> [collection to rebuild indexes]
package main

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/sourcenetwork/defradb/client/options"
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
	if len(args) < 3 {
		return fmt.Errorf("usage: kvsnap export <rootdir> <collection> <out> [docID ...] | import <rootdir> <in> <mapping|-> [collection]")
	}
	n, err := open(ctx, args[1])
	if err != nil {
		return err
	}
	defer n.DB.Close()
	switch args[0] {
	case "export":
		if len(args) < 4 {
			return fmt.Errorf("export needs <rootdir> <collection> <out>")
		}
		f, err := os.Create(args[3])
		if err != nil {
			return err
		}
		defer func() { _ = f.Close() }()
		count, err := n.DB.ExportDocKVs(ctx, args[2], args[4:], f, false)
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
	case "import":
		if len(args) < 4 {
			return fmt.Errorf("import needs <rootdir> <in> <mapping|->")
		}
		f, err := os.Open(args[2])
		if err != nil {
			return err
		}
		defer func() { _ = f.Close() }()
		var count int
		if args[3] == "-" {
			count, err = n.DB.ImportRawKVs(ctx, f)
		} else {
			mapping, rerr := os.ReadFile(args[3])
			if rerr != nil {
				return rerr
			}
			count, err = n.DB.ImportRawKVsWithMapping(ctx, f, mapping)
		}
		if err != nil {
			return err
		}
		if len(args) > 4 {
			if err := n.DB.RebuildCollectionIndexes(ctx, args[4]); err != nil {
				return err
			}
		}
		fmt.Println(count)
	default:
		return fmt.Errorf("unknown command %q", args[0])
	}
	return nil
}
