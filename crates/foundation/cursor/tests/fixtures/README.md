# Cursor Fixtures

`all.json` holds tokens in the format of Go's cursor `internal/cursor.Encode`:
`CursorPayload{d, k omitempty}` marshalled with `encoding/json` and encoded
with `base64.RawURLEncoding`. Token strings must be byte-identical to what that
function produces.

## Regenerating

Run a Go program that declares the same `CursorPayload` struct, marshals each
case with `json.Marshal`, and prints `base64.RawURLEncoding.EncodeToString` of
the bytes next to the JSON.
