// The C signature the library's replication filter callback has, for the Go
// function cgo exports with plain char pointers.
#include <stdint.h>
#include "_cgo_export.h"

int defraSDKReplicationFilterShim(uintptr_t context, const char *collection_id,
                                  const char *doc_id, const char *fields_json) {
    return defraSDKReplicationFilter(context, (char *)collection_id, (char *)doc_id,
                                     (char *)fields_json);
}
