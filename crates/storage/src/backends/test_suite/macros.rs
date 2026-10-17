/// Macro to generate test functions for a specific store type
#[macro_export]
macro_rules! generate_backend_tests {
    ($store_fn:expr) => {
        use $crate::backends::test_suite;

        #[test]
        fn shared_test_basic_set_get() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_basic_set_get(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_delete() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_delete(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_has() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_has(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_read_your_writes() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_read_your_writes(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_empty_key_rejected() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_empty_key_rejected(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_readonly_transaction() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_readonly_transaction(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_closed_store_rejected() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_closed_store_rejected(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_discard_prevents_persistence() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_discard_prevents_persistence(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_success_callback() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_success_callback(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_discard_callback() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_discard_callback(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_async_success_callback() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_async_success_callback(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_callback_panic_propagates() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_callback_panic_propagates(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_async_callback_panic_propagates() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_async_callback_panic_propagates(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_discard_callback_panic_propagates() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_discard_callback_panic_propagates(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_basic() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_basic(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_prefix() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_range() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_range(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_keys_only() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_keys_only(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_closed() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_closed(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_empty_store() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_empty_store(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_start_equals_end() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_start_equals_end(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_prefix_no_match() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_no_match(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_sees_pending_writes() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_sees_pending_writes(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_sees_pending_deletes() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_sees_pending_deletes(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_pending_writes_at_chunk_boundary() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_pending_writes_at_chunk_boundary(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_bounded_scan_across_chunks() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_bounded_scan_across_chunks(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_prefix_scan_across_chunks() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_scan_across_chunks(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_with_prefix() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_with_prefix(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_binary_data() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_binary_data(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_binary_key_ordering() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_binary_key_ordering(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_get_size() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_get_size(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_get_size_with_deletes() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_get_size_with_deletes(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_with_bounds() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_with_bounds(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_single_item_at_start() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_single_item_at_start(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_item_at_end_excluded() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_item_at_end_excluded(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_prefix_between_keys() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_between_keys(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_empty_prefix() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_empty_prefix(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_prefix_with_start() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_with_start(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_multiple_iterators() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_multiple_iterators(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_seek() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_seek_between_keys() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_between_keys(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_seek_past_end() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_past_end(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reset() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_seek_after_iteration() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_after_iteration(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_seek_reset_on_closed() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_reset_on_closed(&store).await;
            })
            .unwrap()
        }

        // Reverse iterator edge case tests (from Go corekv)
        #[test]
        fn shared_test_iterator_reverse_start_only() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_start_only(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_end_only() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_end_only(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_end_single_item_out_of_bounds() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_end_single_item_out_of_bounds(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_start_end_no_items_in_range() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_start_end_no_items_in_range(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_seek() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_seek(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_seek_next() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_seek_next(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_end_seek() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_end_seek(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reverse_prefix_next() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_prefix_next(&store).await;
            })
            .unwrap()
        }

        // Iterator state transition tests (from Go corekv)
        #[test]
        fn shared_test_iterator_reset_partial_iteration() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset_partial_iteration(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reset_after_exhaustion() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset_after_exhaustion(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_reset_then_seek() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset_then_seek(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_seek_respects_start_bound() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_respects_start_bound(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_multiple_resets() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_multiple_resets(&store).await;
            })
            .unwrap()
        }

        // Empty value handling tests
        #[test]
        fn shared_test_empty_value_handling() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_empty_value_handling(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_empty_values() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_empty_values(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_inverted_bounds_yield_empty() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_inverted_bounds_yield_empty(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_inverted_bounds_yield_empty_reverse() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_inverted_bounds_yield_empty_reverse(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_start_beyond_prefix_yields_empty() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_start_beyond_prefix_yields_empty(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_iterator_inverted_bounds_with_pending_writes() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_iterator_inverted_bounds_with_pending_writes(&store).await;
            })
            .unwrap()
        }
    };
}

/// Macro for concurrency tests that need Arc<Store>
/// NOTE: This macro assumes generate_backend_tests! was already called, which imports test_suite
#[macro_export]
macro_rules! generate_backend_concurrency_tests {
    ($arc_store_fn:expr) => {
        #[test]
        fn shared_test_concurrent_writes_different_keys() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_concurrent_writes_different_keys(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_concurrent_writes_same_key() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_concurrent_writes_same_key(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_last_writer_wins() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_last_writer_wins(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_last_writer_wins_reverse() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_last_writer_wins_reverse(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_parallel_stress() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_parallel_stress(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_snapshot_isolation_concurrent() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_snapshot_isolation_concurrent(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_snapshot_isolation_long_running_reader() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_snapshot_isolation_long_running_reader(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_snapshot_isolation_iterator() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_snapshot_isolation_iterator(store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_write_write_isolation() {
            workers::block_on(async {
                let store = $arc_store_fn().await;
                test_suite::test_write_write_isolation(store).await;
            })
            .unwrap()
        }
    };
}

/// Macro for Dropable tests (for stores that implement the Dropable trait)
/// NOTE: This macro assumes generate_backend_tests! was already called, which imports test_suite
#[macro_export]
macro_rules! generate_backend_dropable_tests {
    ($store_fn:expr) => {
        #[test]
        fn shared_test_drop_all() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_drop_all(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_drop_all_then_write() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_drop_all_then_write(&store).await;
            })
            .unwrap()
        }

        #[test]
        fn shared_test_drop_all_empty_store() {
            workers::block_on(async {
                let store = $store_fn().await;
                test_suite::test_drop_all_empty_store(&store).await;
            })
            .unwrap()
        }
    };
}

pub use generate_backend_concurrency_tests;
pub use generate_backend_dropable_tests;
pub use generate_backend_tests;
