//! The descriptions the GraphQL schema shows clients, taken verbatim from
//! Go DefraDB so introspection reads the same on both.

pub(super) const AVG: &str = "\nReturns the average of the specified field values within the specified child sets. If\n multiple fields/sets are specified, the combined average of all items within each set\n (true average, not an average of averages) will be returned as a single value.\n";
pub(super) const COUNT: &str = "\nReturns the total number of items within the specified child sets. If multiple child\n sets are specified, the combined total of all of them will be returned as a single value.\n";
pub(super) const GROUP_FIELD: &str = "\nThe group field may be used to return a set of records belonging to the group.\n It must be used alongside a 'groupBy' argument on the parent selector. It may\n contain any field on the type being grouped, including those used by the\n groupBy.\n";
pub(super) const DOC_IDS_ARG: &str = "\nAn optional set of docIDs for this field. Only documents with a docID\n matching a docID in the given set will be returned.  If no documents match,\n the result will be null/empty. If an empty set is provided, this argument will\n be ignored.\n";
pub(super) const LIST_FIELD_FILTER_ARG: &str = "\nAn optional filter for this join, if none of the related records meet the filter\n criteria the host record will still be returned, but the value of this field will\n be empty.\n";
pub(super) const SINGLE_FIELD_FILTER_ARG: &str = "\nAn optional filter for this join, if the related record does\n not meet the filter criteria the host record will still be returned,\n but the value of this field will be null.\n";
pub(super) const GROUP_BY_ARG: &str = "\nAn optional set of fields for which to group the contents of this field by.\n If this argument is provided, only fields used to group may be rendered in\n the immediate child selector.  Additional fields may be selected by using\n the 'GROUP' selector within the immediate child selector. If an empty set\n is provided, the restrictions mentioned still apply, although all results\n will appear within the same group.\n";
pub(super) const LIMIT_ARG: &str = "\nAn optional value that caps the number of results to the number provided.\n A limit of zero will be ignored.\n";
pub(super) const OFFSET_ARG: &str = "\nAn optional value that skips the given number of results that would have\n otherwise been returned.  Commonly used alongside the 'limit' argument,\n this argument will still work on its own.\n";
pub(super) const ORDER_ARG: &str = "\nAn optional set of field-orders which may be used to sort the results. An\n empty set will be ignored.\n";
pub(super) const MAX: &str = "\nReturns the maximum of the specified field values within the specified child sets. If\n multiple fields/sets are specified, the combined maximum of all items within each set\n will be returned as a single value.\n";
pub(super) const MIN: &str = "\nReturns the minimum of the specified field values within the specified child sets. If\n multiple fields/sets are specified, the combined minimum of all items within each set\n will be returned as a single value.\n";
pub(super) const SUM: &str = "\nReturns the total sum of the specified field values within the specified child sets. If\n multiple fields/sets are specified, the combined sum of all of them will be returned as\n a single value.\n";
pub(super) const DELETED_FIELD: &str =
    "\nIndicates as to whether or not this document has been deleted.\n";
pub(super) const DOC_ID_FIELD: &str =
    "\nThe immutable identifier/docID (primary key) value for this document.\n";
pub(super) const VERSION_FIELD: &str = "\nReturns the head commit for this document.\n";
pub(super) const ALIAS_OPERATOR: &str =
    "The alias operator allows filters to target aliased fields.";
pub(super) const AND_OPERATOR: &str = "\nThe and operator - all checks within this clause must pass in order for this check to pass.\n";
pub(super) const NOT_OPERATOR: &str =
    "\nThe negative operator - this check will only pass if all checks within it fail.\n";
pub(super) const OR_OPERATOR: &str = "\nThe or operator - only one check within this clause must pass in order for this check to pass.\n";
pub(super) const AGGREGATE_FILTER_ARG: &str = "\nAn optional filter for this aggregate, only documents matching the given criteria\n will be aggregated.\n";
pub(super) const COMMIT: &str = "\nCommit represents an individual commit to a MerkleCRDT, every mutation to a\n document will result in a new commit per modified field, and one composite\n commit composed of the field level commits and, in the case of an update,\n the prior composite commit.  If the collection is branchable, there will\n also be a collection-level commit for each mutation.\n";
pub(super) const COMMIT_CID: &str = "\nThe unique CID of this commit, and the primary means through which to safely identify\n a specific commit.\n";
pub(super) const COMMIT_COLLECTION_VERSION_ID: &str = "\nThe ID of the collection version that this commit was committed against. This ID allows one\n to determine the state of the data model at the time of commit.\n";
pub(super) const COMMIT_DELTA: &str =
    "\nThe CBOR encoded representation of the value that is saved as part of this commit.\n";
pub(super) const COMMIT_DOC_ID: &str = "\nThe docID of the document that this commit is for.\n";
pub(super) const COMMIT_FIELD_NAME: &str = "\nThe name of the field that this commit was committed against. The value is \"_C\" for composite\n blocks and null for collection blocks.\n";
pub(super) const COMMIT_LINKS: &str = "\nChild commits in the DAG that contribute to the composition of this commit.\n Composite commits will link to the field commits for the fields modified during\n the single mutation.  Collection commits will link to composites.\n";
pub(super) const COMMIT_CID_ARG: &str = "\nAn optional value that specifies the commit IDs of the commits to return. If a\n matching commit is not found then an empty set will be returned.\n";
pub(super) const COMMIT_DOC_ID_ARG: &str = "\nAn optional docID parameter for this commit query. Only commits for a document\n with a matching docID will be returned.  If no documents match, the result\n set will be empty.\n";
pub(super) const COMMIT_FILTER_ARG: &str = "Filter results based on specified conditions.";
pub(super) const COMMIT_HEIGHT: &str = "\nHeight represents the location of the commit in the DAG. All commits (collection, composite,\n and field level) on add will have a height of '1', each subsequent local update\n will increment this by one for the new commits.\n";
pub(super) const COMMIT_SIGNATURE: &str = "\nThe signature of the commit, if one exists. This is used to verify the integrity\n of the commit and the data it contains.\n";
pub(super) const COMMITS_FIELD_NAME_FILTER: &str = "Filter operators for commit fieldName.";
pub(super) const EQ_OPERATOR: &str =
    "\nThe equality operator - if the target matches the value the check will pass.\n";
pub(super) const IN_OPERATOR: &str =
    "\nThe contains operator - if the target value is within the given set the check will pass.\n";
pub(super) const NIN_OPERATOR: &str = "\nThe does not contains operator - if the target value is not within the given set the\n check will pass.\n";
pub(super) const COMMITS_FILTER: &str = "Filter argument for commits query.";
pub(super) const COMMITS_FILTER_FIELD_NAME: &str = "Filter commits by field name. Use \"_C\" for document composite commits, the field name (e.g. \"age\") for field commits, or null for collection commits on branchable collections.";
pub(super) const FLOAT64_SCALAR: &str = "The `Float64` scalar type represents signed double-precision fractional values as specified by [IEEE 754](http://en.wikipedia.org/wiki/IEEE_floating_point). ";
pub(super) const GEQ_OPERATOR: &str = "\nThe greater than or equal to operator - if the target value is greater than or equal to the\n given value the check will pass.\n";
pub(super) const GT_OPERATOR: &str = "\nThe greater than operator - if the target value is greater than the given value the\n check will pass.\n";
pub(super) const LEQ_OPERATOR: &str = "\nThe less than or equal to operator - if the target value is less than or equal to the\n given value the check will pass.\n";
pub(super) const LT_OPERATOR: &str = "\nThe less than operator - if the target value is less than the given value the check will pass.\n";
pub(super) const NEQ_OPERATOR: &str =
    "\nThe inequality operator - if the target does not matches the value the check will pass.\n";
pub(super) const JSON_SCALAR: &str = "The `JSON` scalar type represents a JSON value.";
pub(super) const ADD_DOCUMENTS: &str =
    "\nAdds one or more documents of this type using the data provided.\n";
pub(super) const UPDATE_DOCUMENTS: &str = "\nUpdates documents in this collection using the data provided. Only documents\n matching any provided criteria will be updated, if no criteria are provided\n the update will be applied to all documents in the collection.\n";
pub(super) const UPDATE_IDS_ARG: &str = "\nAn optional set of docID values that will limit the update to documents\n with a matching docID. If no matching documents are found, the operation will\n succeed, but no documents will be updated.\n";
pub(super) const UPDATE_INPUT_ARG: &str = "Update field values";
pub(super) const DELETE_DOCUMENTS: &str = "\nDeletes documents in this collection matching any provided criteria. If no\n criteria are provided all documents in the collection will be deleted.\n";
pub(super) const DELETE_IDS_ARG: &str = "\nAn optional set of docID values that will limit the delete to documents with\n a matching docID. If no matching documents are found, the operation will\n succeed, but no documents will be deleted. If an empty set is provided, no\n documents will be deleted.\n";
pub(super) const DELETE_FILTER_ARG: &str = "\nAn optional filter for this delete that will limit the delete to documents\n matching the given criteria. If no matching documents are found, the operation\n will succeed, but no documents will be deleted.\n";
pub(super) const TRUNCATE_DOCUMENTS: &str =
    "\nRemove all or matching documents from this node. Returns true when complete.\n";
pub(super) const TRUNCATE_FILTER_ARG: &str = "\nAn optional filter for this truncate that will limit the truncate to documents\n matching the given criteria. If no matching documents are found, the operation\n will succeed, but no documents will be removed.\n";
pub(super) const ORDER_ASC: &str =
    "\nSort the results in ascending order, e.g. null,1,2,3,a,b,c.\n";
pub(super) const ORDER_DESC: &str =
    "\nSort the results in descending order, e.g. c,b,a,3,2,1,null.\n";
pub(super) const CID_ARG: &str = "\nAn optional value that specifies the commit ID of a document or a branchable collection.\n This CID does not need to be the most recent. If it corresponds to an older version\n the document(s) will be returned at the state they were in at the time of that commit.\n If a matching commit is not found then an empty set will be returned.\n";
pub(super) const SELECT_FILTER_ARG: &str = "\nAn optional filter for this select, only documents matching the given criteria\n will be returned.\n";
pub(super) const SHOW_DELETED_ARG: &str = "\nAn optional value that specifies as to whether deleted documents may be\n returned. This argument will propagate down through any child selects/joins.\n";
pub(super) const COMMITS_QUERY: &str = "\nReturns a set of commits matching any provided criteria. If no arguments are\n provided all commits in the system will be returned.\n";
pub(super) const COMMITS_DEPTH_ARG: &str = "\nAn optional value that specifies the maximum depth to which the commit DAG graph\n should be traversed from matching commits.\n";
pub(super) const SIGNATURE_IDENTITY: &str = "The identity of the signer, which is used to determine the public key used to verify the signature.";
pub(super) const SIGNATURE_TYPE: &str = "The type of the signature, which is used to determine the algorithm used to generate the signature.";
pub(super) const SIGNATURE_VALUE: &str = "The value of the signature, which is used to verify the integrity of the commit and the data it contains.";
pub(super) const ILIKE_OPERATOR: &str = "\nThe case insensitive like operator - if the target value contains the given case insensitive sub-string the check\n will pass. '%' characters may be used as wildcards, for example '_like: \"%ritchie\"' would match on strings\n ending in 'Ritchie'.\n";
pub(super) const LIKE_OPERATOR: &str = "\nThe like operator - if the target value contains the given sub-string the check will pass. '%'\n characters may be used as wildcards, for example '_like: \"%Ritchie\"' would match on strings\n ending in 'Ritchie'.\n";
pub(super) const NILIKE_OPERATOR: &str = "\nThe case insensitive not-like operator - if the target value does not contain the given case insensitive sub-string\n the check will pass. '%' characters may be used as wildcards, for example '_nlike: \"%ritchie\"' would match on\n the string 'Quentin Tarantino'.\n";
pub(super) const NLIKE_OPERATOR: &str = "\nThe not-like operator - if the target value does not contain the given sub-string the check will\n pass. '%' characters may be used as wildcards, for example '_nlike: \"%Ritchie\"' would match on\n the string 'Quentin Tarantino'.\n";
pub(super) const COMMIT_FIELDS: &str =
    "\nThese are the set of fields supported for grouping by in a _commits query.\n";
pub(super) const ANY_OPERATOR: &str = "\nThe any operator - only one check within this clause must pass on each item in order for this check to pass.\t\n";
pub(super) const ALL_OPERATOR: &str = "\nThe all operator - all checks within this clause must pass on each item in order for this check to pass.\t\n";
pub(super) const NONE_OPERATOR: &str = "\nThe none operator - only one check within this clause must fail on one item in order for this check to pass.\t\n";
pub(super) const DATETIME_SCALAR: &str = "The `DateTime` scalar type represents a DateTime. The DateTime is serialized as an RFC 3339 quoted string. A literal value of `UTC_NOW` is supported, and will be serialized  as the current UTC time. Note that this will be resolved as the UTC time on the server's side. It may not exactly match the resolution of time.Now().UTC() on the client's machine at the time of sending the request.";
pub(super) const BLOB_SCALAR: &str = "The `Blob` scalar type represents a binary large object.";
pub(super) const FLOAT32_SCALAR: &str = "The `Float32` scalar type represents signed single-precision fractional values as specified by [IEEE 754](http://en.wikipedia.org/wiki/IEEE_floating_point). ";
pub(super) const ENCRYPT_ARG: &str = "\nEncrypt flag specifies if the input document(s) needs to be encrypted. If set, \n DefraDB will generate a symmetric key for encryption using AES-GCM, and will \n use it to encrypt all fields' values.\n";
pub(super) const ENCRYPT_FIELDS_ARG: &str = "\nAn optional list of individual fields that should be encrypted. For every field\n in the list DefraDB will generate a symmetric key for encryption using AES-GCM.\n If 'encrypt' is set to true, it all fields not listed in 'encryptedFields' will\n be encrypted with the same key.\n";
pub(super) const UPSERT_DOCUMENTS: &str = "\nUpdate or add a document in this collection using the data provided. The provided filter\n must match at most one document. The matching document will be updated with the provided\n update input, or if no matching document is found, a new document will be added with the\n provided add input.\n\nNOTE: It is highly recommended to add an index on the fields used to filter.";
pub(super) const UPSERT_FILTER_ARG: &str = "\nA required filter for this upsert that must match one or zero documents.\n If a matching document is found it will be updated, otherwise a new\n document will be added.\n";
pub(super) const UPDATE_FILTER_ARG: &str = "\nAn optional filter for this update that will limit the update to the documents\n matching the given criteria. If no matching documents are found, the operation\n will succeed, but no documents will be updated.\n";
pub(super) const UPSERT_ADD_ARG: &str = "Add field values";
