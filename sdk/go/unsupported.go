package defra

// Go DefraDB client methods the Rust SDK does not offer yet: each returns
// ErrNotSupported.

import (
	"context"

	"github.com/sourcenetwork/defradb/acp/identity"
	"github.com/sourcenetwork/defradb/client"
	"github.com/sourcenetwork/defradb/client/options"
	"github.com/sourcenetwork/defradb/crypto"
	"github.com/sourcenetwork/immutable"
	lensmodel "github.com/sourcenetwork/lens/host-go/config/model"
)

func (s *store) AddDACActorRelationship(
	ctx context.Context,
	collectionName string,
	docID string,
	relation string,
	targetActor string,
	opts ...options.Enumerable[options.AddDACActorRelationshipOptions],
) (_0 client.AddActorRelationshipResult, err error) {
	err = errNotSupported("AddDACActorRelationship")
	return
}

func (s *store) AddDACPolicy(
	ctx context.Context,
	policy string,
	opts ...options.Enumerable[options.AddDACPolicyOptions],
) (_0 client.AddPolicyResult, err error) {
	err = errNotSupported("AddDACPolicy")
	return
}

func (s *store) AddNACActorRelationship(
	ctx context.Context,
	relation string,
	targetActor string,
	opts ...options.Enumerable[options.AddNACActorRelationshipOptions],
) (_0 client.AddActorRelationshipResult, err error) {
	err = errNotSupported("AddNACActorRelationship")
	return
}

func (s *store) AddP2PDocuments(
	ctx context.Context,
	docIDs []string,
	opts ...options.Enumerable[options.AddP2PDocumentsOptions],
) (err error) {
	err = errNotSupported("AddP2PDocuments")
	return
}

func (s *store) BasicExport(ctx context.Context, filepath string, opts ...options.Enumerable[options.BasicExportOptions]) (err error) {
	err = errNotSupported("BasicExport")
	return
}

func (s *store) BasicImport(ctx context.Context, filepath string) (err error) {
	err = errNotSupported("BasicImport")
	return
}

func (s *store) DeleteCollection(
	ctx context.Context,
	names []string,
	opts ...options.Enumerable[options.DeleteCollectionOptions],
) (err error) {
	err = errNotSupported("DeleteCollection")
	return
}

func (s *store) DeleteDACActorRelationship(
	ctx context.Context,
	collectionName string,
	docID string,
	relation string,
	targetActor string,
	opts ...options.Enumerable[options.DeleteDACActorRelationshipOptions],
) (_0 client.DeleteActorRelationshipResult, err error) {
	err = errNotSupported("DeleteDACActorRelationship")
	return
}

func (s *store) DeleteNACActorRelationship(
	ctx context.Context,
	relation string,
	targetActor string,
	opts ...options.Enumerable[options.DeleteNACActorRelationshipOptions],
) (_0 client.DeleteActorRelationshipResult, err error) {
	err = errNotSupported("DeleteNACActorRelationship")
	return
}

func (s *store) DeleteP2PDocuments(
	ctx context.Context,
	docIDs []string,
	opts ...options.Enumerable[options.DeleteP2PDocumentsOptions],
) (err error) {
	err = errNotSupported("DeleteP2PDocuments")
	return
}

func (s *store) DeleteReplicator(
	ctx context.Context,
	id string,
	opts ...options.Enumerable[options.DeleteReplicatorOptions],
) (err error) {
	err = errNotSupported("DeleteReplicator")
	return
}

func (s *store) DisableNAC(ctx context.Context, opts ...options.Enumerable[options.DisableNACOptions]) (err error) {
	err = errNotSupported("DisableNAC")
	return
}

func (s *store) Disconnect(ctx context.Context, addresses []string, opts ...options.Enumerable[options.DisconnectOptions]) (err error) {
	err = errNotSupported("Disconnect")
	return
}

func (s *store) GetNACStatus(ctx context.Context, opts ...options.Enumerable[options.GetNACStatusOptions]) (_0 client.NACStatusResult, err error) {
	err = errNotSupported("GetNACStatus")
	return
}

func (s *store) GetNodeIdentity(ctx context.Context) (_0 immutable.Option[identity.PublicRawIdentity], err error) {
	err = errNotSupported("GetNodeIdentity")
	return
}

func (s *store) ListActions(ctx context.Context, opts ...options.Enumerable[options.ListActionsOptions]) (_0 []client.ActionExecution, err error) {
	err = errNotSupported("ListActions")
	return
}

func (s *store) ListAllEncryptedIndexes(
	context.Context,
	...options.Enumerable[options.ListAllEncryptedIndexesOptions],
) (_0 map[client.CollectionName][]client.EncryptedIndexDescription, err error) {
	err = errNotSupported("ListAllEncryptedIndexes")
	return
}

func (s *store) ListIndexes(
	ctx context.Context,
	opts ...options.Enumerable[options.ListIndexesOptions],
) (_0 map[client.CollectionName][]client.ListIndexesResult, err error) {
	err = errNotSupported("ListIndexes")
	return
}

func (s *store) ListLenses(ctx context.Context, opts ...options.Enumerable[options.ListLensesOptions]) (_0 map[string]lensmodel.Lens, err error) {
	err = errNotSupported("ListLenses")
	return
}

func (s *store) ListP2PCollections(
	ctx context.Context,
	opts ...options.Enumerable[options.ListP2PCollectionsOptions],
) (_0 []string, err error) {
	err = errNotSupported("ListP2PCollections")
	return
}

func (s *store) ListP2PDocuments(ctx context.Context, opts ...options.Enumerable[options.ListP2PDocumentsOptions]) (_0 []string, err error) {
	err = errNotSupported("ListP2PDocuments")
	return
}

func (s *store) ListReplicators(ctx context.Context, opts ...options.Enumerable[options.ListReplicatorsOptions]) (_0 []client.Replicator, err error) {
	err = errNotSupported("ListReplicators")
	return
}

func (s *store) PatchCollection(
	ctx context.Context,
	patch string,
	migration immutable.Option[lensmodel.Lens],
	opts ...options.Enumerable[options.PatchCollectionOptions],
) (err error) {
	err = errNotSupported("PatchCollection")
	return
}

func (s *store) PrintDump(ctx context.Context) (err error) {
	err = errNotSupported("PrintDump")
	return
}

func (s *store) ReEnableNAC(ctx context.Context, opts ...options.Enumerable[options.ReEnableNACOptions]) (err error) {
	err = errNotSupported("ReEnableNAC")
	return
}

func (s *store) RefreshViews(ctx context.Context, opts ...options.Enumerable[options.RefreshViewsOptions]) (err error) {
	err = errNotSupported("RefreshViews")
	return
}

func (s *store) SetActiveCollectionVersion(
	ctx context.Context,
	versionID string,
	opts ...options.Enumerable[options.SetActiveCollectionVersionOptions],
) (err error) {
	err = errNotSupported("SetActiveCollectionVersion")
	return
}

func (s *store) SetMigration(
	ctx context.Context,
	config client.LensConfig, opts ...options.Enumerable[options.SetMigrationOptions],
) (_0 string, err error) {
	err = errNotSupported("SetMigration")
	return
}

func (s *store) SyncBranchableCollection(
	ctx context.Context,
	collectionID string,
	opts ...options.Enumerable[options.SyncBranchableCollectionOptions],
) (err error) {
	err = errNotSupported("SyncBranchableCollection")
	return
}

func (s *store) SyncCollectionVersions(
	ctx context.Context,
	versionIDs []string,
	opts ...options.Enumerable[options.SyncCollectionVersionsOptions],
) (err error) {
	err = errNotSupported("SyncCollectionVersions")
	return
}

func (s *store) SyncDocuments(
	ctx context.Context,
	collectionName string,
	docIDs []string,
	opts ...options.Enumerable[options.SyncDocumentsOptions],
) (err error) {
	err = errNotSupported("SyncDocuments")
	return
}

func (s *store) VerifySignature(
	ctx context.Context,
	blockCid string,
	pubKey crypto.PublicKey,
	opts ...options.Enumerable[options.VerifySignatureOptions],
) (err error) {
	err = errNotSupported("VerifySignature")
	return
}

func (c *Collection) DeleteDocument(
	ctx context.Context,
	docID client.DocID, opts ...options.Enumerable[options.DeleteDocumentOptions],
) (_0 bool, err error) {
	err = errNotSupported("DeleteDocument")
	return
}

func (c *Collection) DeleteEncryptedIndex(
	ctx context.Context,
	fieldName string,
	opts ...options.Enumerable[options.DeleteEncryptedIndexOptions],
) (err error) {
	err = errNotSupported("DeleteEncryptedIndex")
	return
}

func (c *Collection) DeleteIndex(
	ctx context.Context,
	indexName string,
	opts ...options.Enumerable[options.DeleteCollectionIndexOptions],
) (err error) {
	err = errNotSupported("DeleteIndex")
	return
}

func (c *Collection) ExistsDocument(
	ctx context.Context,
	docID client.DocID, opts ...options.Enumerable[options.ExistsDocumentOptions],
) (_0 bool, err error) {
	err = errNotSupported("ExistsDocument")
	return
}

func (c *Collection) ListEncryptedIndexes(
	ctx context.Context,
	opts ...options.Enumerable[options.ListCollectionEncryptedIndexesOptions],
) (_0 []client.EncryptedIndexDescription, err error) {
	err = errNotSupported("ListEncryptedIndexes")
	return
}

func (c *Collection) ListIndexes(
	ctx context.Context,
	opts ...options.Enumerable[options.ListCollectionIndexesOptions],
) (_0 []client.ListIndexesResult, err error) {
	err = errNotSupported("ListIndexes")
	return
}

func (c *Collection) NewEncryptedIndex(
	ctx context.Context,
	desc client.EncryptedIndexDescription, opts ...options.Enumerable[options.NewEncryptedIndexOptions],
) (_0 client.EncryptedIndexDescription, err error) {
	err = errNotSupported("NewEncryptedIndex")
	return
}

func (c *Collection) NewIndex(
	context.Context,
	client.NewIndexRequest, ...options.Enumerable[options.NewCollectionIndexOptions],
) (_0 client.IndexDescription, err error) {
	err = errNotSupported("NewIndex")
	return
}

func (c *Collection) UpdateDocumentsWithFilter(
	ctx context.Context,
	filter any,
	updater string,
	opts ...options.Enumerable[options.UpdateDocumentsWithFilterOptions],
) (_0 *client.UpdateResult, err error) {
	err = errNotSupported("UpdateDocumentsWithFilter")
	return
}
