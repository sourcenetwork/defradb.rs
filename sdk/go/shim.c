// The C signatures the library's replication filter and retention callbacks have, for the Go
// functions cgo exports with plain char pointers.
#include <stdint.h>
#include "_cgo_export.h"

int defraSDKReplicationFilterShim(uintptr_t context, const char *collection_id,
                                  const char *doc_id, const char *fields_json) {
    return defraSDKReplicationFilter(context, (char *)collection_id, (char *)doc_id,
                                     (char *)fields_json);
}

int defraSDKRetentionRuleShim(uintptr_t context, const char *collection_id, char *field_out,
                              size_t field_capacity, int64_t *floor_out) {
    return defraSDKRetentionRule(context, (char *)collection_id, field_out, field_capacity,
                                 floor_out);
}
