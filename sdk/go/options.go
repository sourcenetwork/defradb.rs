package defra

import (
	"errors"
	"fmt"

	"github.com/sourcenetwork/defradb/acp/identity"
	"github.com/sourcenetwork/immutable"
	"github.com/sourcenetwork/immutable/enumerable"
)

// ErrNotSupported is what a `client` method the Rust SDK does not offer yet
// returns.
var ErrNotSupported = errors.New("not supported by the Rust SDK")

func errNotSupported(method string) error {
	return fmt.Errorf("defra: %s: %w", method, ErrNotSupported)
}

// newOptions applies Go DefraDB's functional options to a zero T.
func newOptions[T any](opts ...enumerable.Enumerable[func(*T)]) *T {
	o := new(T)
	applyOptions(o, opts...)
	return o
}

// applyOptions applies Go DefraDB's functional options to o.
func applyOptions[T any](o *T, opts ...enumerable.Enumerable[func(*T)]) {
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
}

// did is the DID an option's identity names, or empty for none.
func did(id immutable.Option[identity.Identity]) string {
	if !id.HasValue() || id.Value() == nil {
		return ""
	}
	return id.Value().DID()
}
