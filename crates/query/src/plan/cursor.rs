//! CursorNode — wraps a child plan with cursor pagination semantics.
//!
//! Sits at the top of a cursor query's plan tree, above the existing scan/
//! filter/order stack. Owns per-row cursor logic: skip-until-after,
//! collect, probe-for-hasNext, encode startCursor/endCursor.

use crate::doc::Doc;
use crate::document::DocumentMapping;
use crate::error::Result;
use crate::mapper::{CursorPageInfoFields, OrderCondition};
use async_trait::async_trait;
use cursor::Cursor;
use std::collections::{BTreeMap, VecDeque};

use crate::planner::{ExecInfo, PlanNode};

/// Direction of cursor pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorState {
    /// Actively collecting rows for the page.
    Collecting,
    /// Scanning rows until we pass the `after` boundary (no index seek).
    SkippingUntilAfter,
    /// All rows have been emitted; subsequent calls return false.
    Drained,
}

/// Cursor keys extracted from a document at emit time (doc_id + order field values).
///
/// Storing only what's needed for cursor encoding avoids keeping a full Doc clone alive
/// solely for page_info computation.
struct CursorSnapshot {
    doc_id: String,
    keys: BTreeMap<String, serde_json::Value>,
}

/// Plan node that implements cursor pagination above any child plan.
pub struct CursorNode {
    inner: Box<dyn PlanNode>,
    direction: CursorDirection,
    /// Page limit from `first`/`last`. `None` means no limit was supplied, in
    /// which case the node returns all remaining rows — mirroring Go's
    /// `cursorNode` where the limit check is gated on `first.HasValue()` /
    /// `last.HasValue()` (omitting the arg returns all results).
    limit: Option<u64>,
    after: Option<Cursor>,
    before: Option<Cursor>,
    page_info_fields: CursorPageInfoFields,
    order_fields: Vec<OrderCondition>,

    state: CursorState,
    /// True once `populate_backward_buffer` has run for the backward path.
    buffer_populated: bool,
    /// Buffer used by the backward path: populated on the first `next()` call.
    buffer: VecDeque<Doc>,
    current_doc: Doc,
    first_snapshot: Option<CursorSnapshot>,
    last_snapshot: Option<CursorSnapshot>,
    has_next: bool,
    has_prev: bool,
    index_seek_active: bool,
    /// A backward seek's boundary key is the order values alone (a unique,
    /// non-null index), so a row at those values sits in the boundary's slot
    /// whatever its DocID.
    boundary_keyed_by_value: bool,
    emitted: u64,
    start_cursor: Option<String>,
    end_cursor: Option<String>,
    exec_info: ExecInfo,
}

impl CursorNode {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        inner: Box<dyn PlanNode>,
        direction: CursorDirection,
        limit: Option<u64>,
        after: Option<Cursor>,
        before: Option<Cursor>,
        page_info_fields: CursorPageInfoFields,
        order_fields: Vec<OrderCondition>,
        index_seek_active: bool,
    ) -> Self {
        // Forward: if the index already positions us past `after`, or there is
        // no `after` token, start collecting immediately.
        // Backward: always start in Collecting; `buffer_populated` tracks whether
        // the first-call buffer fill has happened.
        let initial_state = match direction {
            CursorDirection::Forward => {
                if index_seek_active || after.is_none() {
                    CursorState::Collecting
                } else {
                    CursorState::SkippingUntilAfter
                }
            }
            CursorDirection::Backward => CursorState::Collecting,
        };
        Self {
            inner,
            direction,
            limit,
            after,
            before,
            page_info_fields,
            order_fields,
            state: initial_state,
            buffer_populated: false,
            buffer: VecDeque::new(),
            current_doc: Doc::default(),
            first_snapshot: None,
            last_snapshot: None,
            has_next: false,
            has_prev: false,
            index_seek_active,
            boundary_keyed_by_value: false,
            emitted: 0,
            start_cursor: None,
            end_cursor: None,
            exec_info: ExecInfo::default(),
        }
    }

    /// Mark the backward seek's boundary key as holding no DocID.
    pub fn with_boundary_keyed_by_value(mut self, keyed_by_value: bool) -> Self {
        self.boundary_keyed_by_value = keyed_by_value;
        self
    }

    /// Return page-info computed after the last `next()` call returned false.
    pub fn page_info_data(&self) -> CursorPageInfo {
        CursorPageInfo {
            has_next: self.has_next,
            has_prev: self.has_prev,
            start_cursor: self.start_cursor.clone(),
            end_cursor: self.end_cursor.clone(),
            fields: self.page_info_fields.clone(),
        }
    }

    /// Extract the cursor snapshot from a doc using the configured order fields.
    ///
    /// Captures only the doc_id and order-field values needed to encode a cursor,
    /// avoiding a full Doc clone for the page_info path.
    fn snapshot_from_doc(
        doc: &Doc,
        document_map: &DocumentMapping,
        order_fields: &[OrderCondition],
    ) -> CursorSnapshot {
        let doc_id = doc.doc_id().unwrap_or("").to_string();
        let mut keys = BTreeMap::new();
        for cond in order_fields {
            if let Some(field_name) = cond.fields.first() {
                if let Some(idx) = document_map.first_index_of_name(field_name) {
                    if let Some(value) = doc.get(idx) {
                        keys.insert(field_name.to_string(), value.clone());
                    }
                }
            }
        }
        CursorSnapshot { doc_id, keys }
    }

    /// Build a `Cursor` from a previously captured snapshot.
    fn build_cursor_from_snapshot(snapshot: &CursorSnapshot) -> Cursor {
        Cursor {
            doc_id: snapshot.doc_id.clone(),
            keys: snapshot.keys.clone(),
        }
    }

    fn finalize_page_info(&mut self) {
        if self.page_info_fields.start_cursor.is_some() {
            if let Some(snap) = self.first_snapshot.as_ref() {
                self.start_cursor = Some(Self::build_cursor_from_snapshot(snap).encode());
            }
        }
        if self.page_info_fields.end_cursor.is_some() {
            if let Some(snap) = self.last_snapshot.as_ref() {
                self.end_cursor = Some(Self::build_cursor_from_snapshot(snap).encode());
            }
        }
    }

    async fn next_forward(&mut self) -> Result<bool> {
        loop {
            match self.state {
                CursorState::Collecting => {
                    if self.limit.is_some_and(|lim| self.emitted >= lim) {
                        // Probe one extra row to determine hasNextPage.
                        self.has_next = self.inner.next().await?;
                        self.has_prev = self.after.is_some();
                        self.state = CursorState::Drained;
                        self.finalize_page_info();
                        return Ok(false);
                    }
                    if self.inner.next().await? {
                        let doc = self.inner.value().deep_clone(); // ONE clone per row
                        let map = self.inner.document_map();
                        if self.first_snapshot.is_none() {
                            self.first_snapshot =
                                Some(Self::snapshot_from_doc(&doc, map, &self.order_fields));
                        }
                        self.last_snapshot =
                            Some(Self::snapshot_from_doc(&doc, map, &self.order_fields));
                        self.current_doc = doc;
                        self.emitted += 1;
                        return Ok(true);
                    } else {
                        self.has_next = false;
                        self.has_prev = self.after.is_some();
                        self.state = CursorState::Drained;
                        self.finalize_page_info();
                        return Ok(false);
                    }
                }
                CursorState::SkippingUntilAfter => {
                    // Slow path (no index seek). Mirrors Go's cursorNode skip
                    // phase: locate the cursor boundary purely by exact docID
                    // equality and rely on the child plan's ordering — no
                    // field-value comparison. Skip rows until the row whose
                    // docID equals the `after` cursor's docID, skip that row
                    // too, then collect everything after it. If the docID is
                    // never found (e.g. the cursor doc was deleted), the page
                    // ends up empty — matching Go. This works for any ORDER BY
                    // because the orderby/scan node below already emits rows in
                    // order; the cursor only needs to find the boundary doc.
                    let after_doc_id = self.after.as_ref().map(|c| c.doc_id.as_str());
                    if self.inner.next().await? {
                        let row_id = self.inner.value().doc_id();
                        if after_doc_id.is_some() && row_id == after_doc_id {
                            // Found the cursor doc — skip it, collect from the next row.
                            self.state = CursorState::Collecting;
                        }
                        // Otherwise keep skipping (stay in SkippingUntilAfter).
                        continue;
                    } else {
                        // Exhausted before finding the boundary.
                        self.state = CursorState::Drained;
                        self.finalize_page_info();
                        return Ok(false);
                    }
                }
                CursorState::Drained => return Ok(false),
            }
        }
    }

    async fn next_backward(&mut self) -> Result<bool> {
        // On the very first call, drain the inner plan into self.buffer.
        if !self.buffer_populated {
            self.populate_backward_buffer().await?;
            self.buffer_populated = true;
        }

        match self.state {
            CursorState::Collecting => match self.buffer.pop_front() {
                Some(doc) => {
                    self.current_doc = doc;
                    Ok(true)
                }
                None => {
                    self.state = CursorState::Drained;
                    Ok(false)
                }
            },
            CursorState::Drained => Ok(false),
            CursorState::SkippingUntilAfter => unreachable!("SkippingUntilAfter is forward-only"),
        }
    }

    /// Drain the inner plan into `self.buffer` according to backward semantics,
    /// then compute snapshots and page-info flags.
    async fn populate_backward_buffer(&mut self) -> Result<()> {
        if self.index_seek_active {
            self.populate_backward_buffer_fast().await?;
        } else {
            self.populate_backward_buffer_slow().await?;
        }
        self.populate_snapshots_after_buffer();
        Ok(())
    }

    /// Fast path: the inner plan already iterates in reverse from the `before`
    /// boundary, its key included. The boundary row, if it is still there,
    /// comes first and is dropped. Collect up to `limit + 1` docs (or all docs
    /// when no limit) and reverse them so callers receive docs in logical
    /// (ascending) order.
    async fn populate_backward_buffer_fast(&mut self) -> Result<()> {
        // +1 to detect has_prev; `None` means unbounded (return all rows).
        let window_size = self.limit.map(|lim| lim as usize + 1);
        let mut collected: Vec<Doc> = Vec::new();
        let mut first = true;
        while self.inner.next().await? {
            let doc = self.inner.value().deep_clone();
            if std::mem::take(&mut first) && self.at_before_boundary(&doc) {
                continue;
            }
            collected.push(doc);
            if window_size.is_some_and(|w| collected.len() >= w) {
                break;
            }
        }
        // Reverse so the buffer holds docs in ascending (logical) order.
        collected.reverse();
        if let Some(lim) = self.limit {
            if collected.len() > lim as usize {
                self.has_prev = true;
                collected.remove(0); // drop the extra doc at the front
            }
        }
        for doc in collected {
            self.buffer.push_back(doc);
        }
        Ok(())
    }

    /// Whether `doc`, the first row of an inclusive backward seek, sits at the
    /// `before` token's index key. `has_next` records whether it is the token's
    /// own row, as Go's `beforeCursorBoundarySurvives` reports: a page before a
    /// deleted or moved row has no next page.
    fn at_before_boundary(&mut self, doc: &Doc) -> bool {
        let Some(before) = self.before.as_ref() else {
            return false;
        };
        let row = Self::snapshot_from_doc(doc, self.inner.document_map(), &self.order_fields);
        if row.keys != before.keys {
            return false;
        }
        let same_row = row.doc_id == before.doc_id;
        self.has_next = same_row;
        same_row || self.boundary_keyed_by_value
    }

    /// Slow path (no index seek). Mirrors Go's `drainBackwardPage`: scan forward
    /// (the child plan emits rows in order), collecting until the `before`
    /// boundary doc — matched by exact docID equality and excluded from output —
    /// while keeping a sliding window of the last `limit + 1` rows (or all rows
    /// when no limit) to bound memory and still detect has_prev.
    ///
    /// No field-value comparison is performed: the child's ordering is
    /// authoritative, so the cursor only needs to find the boundary doc by its
    /// unique docID. This is correct for any ORDER BY (including legacy
    /// docID-only cursors against a field-ordered query).
    async fn populate_backward_buffer_slow(&mut self) -> Result<()> {
        let before_doc_id = self.before.as_ref().map(|c| c.doc_id.clone());
        // +1 to detect has_prev; `None` means unbounded (return all rows).
        let window_size = self.limit.map(|lim| lim as usize + 1);
        while self.inner.next().await? {
            let value = self.inner.value();
            if let Some(boundary) = before_doc_id.as_deref() {
                // The `before` doc itself is excluded; stop on exact match.
                // Finding it is what makes a next page (Go's drainBackwardPage).
                if value.doc_id() == Some(boundary) {
                    self.has_next = true;
                    break;
                }
            }
            let doc = value.deep_clone();
            self.buffer.push_back(doc);
            if window_size.is_some_and(|w| self.buffer.len() > w) {
                self.buffer.pop_front();
            }
        }
        if let Some(lim) = self.limit {
            if self.buffer.len() > lim as usize {
                self.has_prev = true;
                self.buffer.pop_front();
            }
        }
        Ok(())
    }

    /// Capture first/last snapshots from the buffer.
    /// Called after either fast or slow population.
    fn populate_snapshots_after_buffer(&mut self) {
        let map = self.inner.document_map();
        if let Some(first) = self.buffer.front() {
            self.first_snapshot = Some(Self::snapshot_from_doc(first, map, &self.order_fields));
        }
        if let Some(last) = self.buffer.back() {
            self.last_snapshot = Some(Self::snapshot_from_doc(last, map, &self.order_fields));
        }
        self.finalize_page_info();
    }
}

/// Page-info emitted once the node is drained.
pub struct CursorPageInfo {
    pub has_next: bool,
    pub has_prev: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
    pub fields: CursorPageInfoFields,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for CursorNode {
    async fn init(&mut self) -> Result<()> {
        self.emitted = 0;
        self.first_snapshot = None;
        self.last_snapshot = None;
        self.has_next = false;
        self.has_prev = false;
        self.start_cursor = None;
        self.end_cursor = None;
        self.buffer.clear();
        self.buffer_populated = false;
        self.state = match self.direction {
            CursorDirection::Forward => {
                if self.index_seek_active || self.after.is_none() {
                    CursorState::Collecting
                } else {
                    CursorState::SkippingUntilAfter
                }
            }
            CursorDirection::Backward => CursorState::Collecting,
        };
        self.exec_info = ExecInfo::default();
        self.inner.init().await
    }

    async fn start(&mut self) -> Result<()> {
        self.inner.start().await
    }

    async fn next(&mut self) -> Result<bool> {
        self.exec_info.iterations += 1;
        match self.direction {
            CursorDirection::Forward => self.next_forward().await,
            CursorDirection::Backward => self.next_backward().await,
        }
    }

    fn value(&self) -> &Doc {
        &self.current_doc
    }

    async fn close(&mut self) -> Result<()> {
        self.inner.close().await
    }

    fn source(&self) -> Option<&dyn PlanNode> {
        Some(self.inner.as_ref())
    }

    fn document_map(&self) -> &DocumentMapping {
        self.inner.document_map()
    }

    fn kind(&self) -> &'static str {
        "cursorNode"
    }

    fn exec_info(&self) -> ExecInfo {
        self.exec_info.clone()
    }

    fn page_info(&self) -> Option<crate::plan::CursorPageInfo> {
        Some(self.page_info_data())
    }

    fn explain_inner(&self) -> serde_json::Value {
        use serde_json::{json, Value};
        let (first, last) = match self.direction {
            CursorDirection::Forward => (self.limit, None),
            CursorDirection::Backward => (None, self.limit),
        };
        let token = |cursor: &Option<Cursor>| cursor.as_ref().map(Cursor::encode);
        let cursor_value = self.after.as_ref().or(self.before.as_ref()).map(|c| {
            let mut value = json!({ "docID": c.doc_id });
            if !c.keys.is_empty() {
                value["keys"] = json!(c.keys);
            }
            value
        });
        // Go Shinzo's `cursorNode.simpleExplain`.
        let mut obj = serde_json::Map::new();
        obj.insert("first".into(), json!(first));
        obj.insert("after".into(), json!(token(&self.after)));
        obj.insert("last".into(), json!(last));
        obj.insert("before".into(), json!(token(&self.before)));
        obj.insert("cursorValue".into(), cursor_value.unwrap_or(Value::Null));
        if let Value::Object(child) = self.inner.explain() {
            obj.extend(child);
        }
        Value::Object(obj)
    }

    fn explain_execute_inner(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "iterations".into(),
            serde_json::json!(self.exec_info.iterations),
        );
        if let serde_json::Value::Object(child) = self.inner.explain_execute() {
            obj.extend(child);
        }
        serde_json::Value::Object(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Doc;
    use crate::document::DocumentMapping;
    use crate::error::Result;
    use crate::mapper::CursorPageInfoFields;
    use async_trait::async_trait;
    use std::collections::VecDeque;

    // --- FakePlan: a PlanNode that yields a preset sequence of Docs ---

    struct FakePlan {
        rows: VecDeque<Doc>,
        current: Doc,
        mapping: DocumentMapping,
    }

    impl FakePlan {
        fn new(rows: Vec<Doc>) -> Self {
            Self {
                rows: rows.into(),
                current: Doc::default(),
                mapping: DocumentMapping::new(),
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl PlanNode for FakePlan {
        async fn init(&mut self) -> Result<()> {
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            Ok(())
        }
        async fn next(&mut self) -> Result<bool> {
            match self.rows.pop_front() {
                Some(doc) => {
                    self.current = doc;
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        fn value(&self) -> &Doc {
            &self.current
        }
        async fn close(&mut self) -> Result<()> {
            Ok(())
        }
        fn source(&self) -> Option<&dyn PlanNode> {
            None
        }
        fn document_map(&self) -> &DocumentMapping {
            &self.mapping
        }
        fn kind(&self) -> &'static str {
            "fakePlan"
        }
    }

    /// Build a Doc with `_docID` set to `id`.
    fn doc_with_id(id: &str) -> Doc {
        let mut doc = Doc::new(1);
        doc.set_doc_id(id);
        doc
    }

    /// Extract the doc_id from the CursorNode's current value.
    fn current_id(node: &CursorNode) -> &str {
        node.value().doc_id().unwrap_or("")
    }

    #[test]
    fn forward_first_only_emits_n_rows() {
        workers::block_on(async {
            let inner = FakePlan::new(vec![
                doc_with_id("a"),
                doc_with_id("b"),
                doc_with_id("c"),
                doc_with_id("d"),
            ]);
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Forward,
                Some(2),
                None, // no after token
                None,
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    ..Default::default()
                },
                vec![],
                false,
            );

            assert!(node.next().await.unwrap(), "first row");
            assert_eq!(current_id(&node), "a");

            assert!(node.next().await.unwrap(), "second row");
            assert_eq!(current_id(&node), "b");

            // Third call should drain and return false.
            assert!(
                !node.next().await.unwrap(),
                "should be done after page_size"
            );

            let info = node.page_info_data();
            assert!(info.has_next, "has_next should be true: c and d remain");
        })
        .unwrap()
    }

    #[test]
    fn backward_last_only_emits_last_n_in_order() {
        workers::block_on(async {
            // No `before` cursor; iterate forward through all, keep last 2.
            let inner = FakePlan::new(vec![
                doc_with_id("a"),
                doc_with_id("b"),
                doc_with_id("c"),
                doc_with_id("d"),
            ]);
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Backward,
                Some(2),
                None,
                None,
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    has_prev: Some("hasPrev".to_string()),
                    ..Default::default()
                },
                vec![],
                false,
            );
            node.init().await.unwrap();

            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "c");
            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "d");
            assert!(!node.next().await.unwrap());

            let info = node.page_info_data();
            assert!(!info.has_next, "before is None => has_next=false");
            assert!(
                info.has_prev,
                "we dropped rows from the front => has_prev=true"
            );
        })
        .unwrap()
    }

    #[test]
    fn backward_last_before_stops_at_boundary() {
        workers::block_on(async {
            // `before` = "c"; collect a, b (window of 2); stop before "c".
            let inner = FakePlan::new(vec![
                doc_with_id("a"),
                doc_with_id("b"),
                doc_with_id("c"),
                doc_with_id("d"),
            ]);
            let before = Cursor::from_doc_id("c");
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Backward,
                Some(2),
                None,
                Some(before),
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    has_prev: Some("hasPrev".to_string()),
                    ..Default::default()
                },
                vec![],
                false,
            );
            node.init().await.unwrap();

            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "a");
            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "b");
            assert!(!node.next().await.unwrap());

            let info = node.page_info_data();
            assert!(info.has_next, "the before row was found => has_next=true");
        })
        .unwrap()
    }

    /// A backward page over `rows` with `last: 2` and `before`, its ids and
    /// `has_next`.
    fn backward_page(
        rows: &[&str],
        before: Cursor,
        index_seek_active: bool,
        keyed_by_value: bool,
    ) -> (Vec<String>, bool) {
        workers::block_on(async {
            let inner = FakePlan::new(rows.iter().map(|id| doc_with_id(id)).collect());
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Backward,
                Some(2),
                None,
                Some(before),
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    ..Default::default()
                },
                vec![],
                index_seek_active,
            )
            .with_boundary_keyed_by_value(keyed_by_value);
            node.init().await.unwrap();
            let mut ids = Vec::new();
            while node.next().await.unwrap() {
                ids.push(current_id(&node).to_string());
            }
            (ids, node.page_info_data().has_next)
        })
        .unwrap()
    }

    #[test]
    fn backward_before_a_missing_row_has_no_next_page() {
        let (ids, has_next) =
            backward_page(&["a", "b", "d"], Cursor::from_doc_id("c"), false, false);
        assert_eq!(ids, ["b", "d"]);
        assert!(!has_next, "the before row is gone => has_next=false");
    }

    #[test]
    fn backward_seek_drops_the_boundary_row_and_has_a_next_page() {
        let (ids, has_next) =
            backward_page(&["c", "b", "a"], Cursor::from_doc_id("c"), true, false);
        assert_eq!(ids, ["a", "b"]);
        assert!(has_next, "the seek met the before row => has_next=true");
    }

    #[test]
    fn backward_seek_past_a_missing_boundary_keeps_every_row() {
        let (ids, has_next) = backward_page(&["b", "a"], Cursor::from_doc_id("c"), true, false);
        assert_eq!(ids, ["a", "b"]);
        assert!(!has_next, "the seek met no before row => has_next=false");
    }

    #[test]
    fn backward_seek_drops_another_row_in_a_value_keyed_boundary_slot() {
        let (ids, has_next) = backward_page(&["x", "b", "a"], Cursor::from_doc_id("c"), true, true);
        assert_eq!(ids, ["a", "b"]);
        assert!(
            !has_next,
            "another row holds the unique key => has_next=false"
        );
    }

    #[test]
    fn backward_seek_keeps_the_before_row_once_it_moved() {
        let mut moved = Cursor::from_doc_id("c");
        moved.keys.insert("age".into(), serde_json::json!(45));
        let (ids, has_next) = backward_page(&["c", "b", "a"], moved, true, false);
        assert_eq!(ids, ["b", "c"]);
        assert!(!has_next, "the row left the boundary key => has_next=false");
    }

    #[test]
    fn forward_first_after_skips_to_boundary() {
        workers::block_on(async {
            let inner = FakePlan::new(vec![
                doc_with_id("a"),
                doc_with_id("b"),
                doc_with_id("c"),
                doc_with_id("d"),
            ]);
            let after = Cursor::from_doc_id("b");
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Forward,
                Some(2),
                Some(after),
                None,
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    has_prev: Some("hasPrev".to_string()),
                    ..Default::default()
                },
                vec![],
                false, // no index seek — slow path
            );

            // Should skip "a" and "b", then emit "c" and "d".
            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "c");

            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "d");

            assert!(!node.next().await.unwrap());

            let info = node.page_info_data();
            assert!(info.has_prev, "has_prev because after token was set");
            assert!(!info.has_next, "has_next false: no rows remain after d");
        })
        .unwrap()
    }

    #[test]
    fn backward_last_index_seek_fast_path_emits_in_order() {
        workers::block_on(async {
            // index_seek_active = true means the inner plan already iterates in reverse.
            // FakePlan with [d, c, b, a] simulates that (inner = reverse logical order).
            let inner = FakePlan::new(vec![
                doc_with_id("d"),
                doc_with_id("c"),
                doc_with_id("b"),
                doc_with_id("a"),
            ]);
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Backward,
                Some(2),
                None,
                None,
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    has_prev: Some("hasPrev".to_string()),
                    ..Default::default()
                },
                vec![],
                true, // index_seek_active — fast path
            );
            node.init().await.unwrap();
            // Buffer: collected reverse = [d, c, b]; len=3 == window_size → break.
            // Reverse for logical order = [b, c, d]; len=3 > page_size=2 → has_prev=true, drop b.
            // Final buffer = [c, d]. Output: c, d.
            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "c");
            assert!(node.next().await.unwrap());
            assert_eq!(current_id(&node), "d");
            assert!(!node.next().await.unwrap());
            let info = node.page_info_data();
            assert!(info.has_prev, "page_size+1 < total => has_prev=true");
            assert!(!info.has_next, "before is None => has_next=false");
        })
        .unwrap()
    }

    #[test]
    fn forward_no_limit_emits_all_rows() {
        workers::block_on(async {
            // `first` omitted (limit None) must return ALL rows, mirroring Go's
            // cursorNode where the limit check is gated on `first.HasValue()`.
            let inner = FakePlan::new(vec![
                doc_with_id("a"),
                doc_with_id("b"),
                doc_with_id("c"),
                doc_with_id("d"),
            ]);
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Forward,
                None, // no `first` — unbounded
                None,
                None,
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    ..Default::default()
                },
                vec![],
                false,
            );

            for expected in ["a", "b", "c", "d"] {
                assert!(node.next().await.unwrap(), "row {expected}");
                assert_eq!(current_id(&node), expected);
            }
            assert!(!node.next().await.unwrap(), "drained after all rows");

            let info = node.page_info_data();
            assert!(
                !info.has_next,
                "no limit => all rows returned => has_next=false"
            );
        })
        .unwrap()
    }

    #[test]
    fn backward_no_limit_emits_all_rows_in_order() {
        workers::block_on(async {
            // `last` omitted (limit None) must return ALL rows in logical order.
            let inner = FakePlan::new(vec![
                doc_with_id("a"),
                doc_with_id("b"),
                doc_with_id("c"),
                doc_with_id("d"),
            ]);
            let mut node = CursorNode::new(
                Box::new(inner),
                CursorDirection::Backward,
                None, // no `last` — unbounded
                None,
                None,
                CursorPageInfoFields {
                    has_next: Some("hasNext".to_string()),
                    has_prev: Some("hasPrev".to_string()),
                    ..Default::default()
                },
                vec![],
                false,
            );
            node.init().await.unwrap();

            for expected in ["a", "b", "c", "d"] {
                assert!(node.next().await.unwrap(), "row {expected}");
                assert_eq!(current_id(&node), expected);
            }
            assert!(!node.next().await.unwrap(), "drained after all rows");

            let info = node.page_info_data();
            assert!(
                !info.has_prev,
                "no limit => no rows dropped => has_prev=false"
            );
        })
        .unwrap()
    }
}
