use std::sync::Arc;

/// The arrival page `options` names, as an anonymous reader sees it.
pub async fn arrivals_page<F>(
    fetcher: &F,
    options: &query::fetcher::DocumentArrivalOptions,
) -> query::fetcher::DocumentArrivalPage
where
    F: query::fetcher::DocFetcher + ?Sized,
{
    let acp = acp::LocalDocumentACP::new(Arc::new(acp::MemoryAcpStore::new()));
    fetcher
        .get_document_arrivals(
            options,
            &query::fetcher::DocumentReader {
                context: &defra_core::RequestContext::default(),
                acp: &acp,
                projection: None,
                identity: &acp::Identity::Anonymous,
            },
        )
        .await
        .unwrap()
}
