# Apply

patch_slop.rs defines a function that implements JSON Patch (RFC 6902).

It takes the following input params:
- A `serde_json::Value`, this is the json patch object
- A map of `schema::CollectionVersion`s keyed by their version id.  If a collection of that version id is not found, it is to be assumed that the user is referencing a new collection by name, instead of version id.

It returns the resultant map of `schema::CollectionVersion`s.
