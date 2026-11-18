# Substitute

Substitute_slop.rs defines a function that takes a json-patch operation that operates on a
map of `schema::CollectionVersion`s keyed by version id and name as per `patch_spec.md`.

The input patch may references values by either their short values (e.g. uint32s), as stored on disk, or
by their human friendly values, as seen via GQL.

The function must replace any given GQL values with their corresponding disk values, for example a scalar
FieldKind given as "Boolean", must be replaced by the short value `2`.

The following substitutions must be supported:
- `[Collection]/Fields/[FieldIndex]/Kind`
- `[Collection]/Fields/[FieldIndex]/Typ`

Additionally, the field index may be referenced by the patch by either the index (as is normal in patch-json), *or* by the field name.
The function must substitute any field-name indexes for the actual numerical index that that name corresponds to.

The Collection may be specified by the user by either version id, or by name.  If specified by name, this function must find a collection with that name and substitute the name in the patch for the version id of the active version of that collection.
