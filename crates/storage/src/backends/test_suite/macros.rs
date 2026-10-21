/// Macro to generate test functions for a specific store type
#[macro_export]
macro_rules! generate_backend_tests {
    ($store_fn:expr) => {
        use $crate::backends::test_suite;

        #[test]
        fn shared_test_basic_set_get() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_basic_set_get(&store).await;
            })
        }

        #[test]
        fn shared_test_delete() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_delete(&store).await;
            })
        }

        #[test]
        fn shared_test_has() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_has(&store).await;
            })
        }

        #[test]
        fn shared_test_read_your_writes() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_read_your_writes(&store).await;
            })
        }

        #[test]
        fn shared_test_empty_key_rejected() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_empty_key_rejected(&store).await;
            })
        }

        #[test]
        fn shared_test_readonly_transaction() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_readonly_transaction(&store).await;
            })
        }

        #[test]
        fn shared_test_closed_store_rejected() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_closed_store_rejected(&store).await;
            })
        }

        #[test]
        fn shared_test_discard_prevents_persistence() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_discard_prevents_persistence(&store).await;
            })
        }

        #[test]
        fn shared_test_success_callback() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_success_callback(&store).await;
            })
        }

        #[test]
        fn shared_test_discard_callback() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_discard_callback(&store).await;
            })
        }

        #[test]
        fn shared_test_async_success_callback() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_async_success_callback(&store).await;
            })
        }

        #[test]
        fn shared_test_callback_panic_propagates() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_callback_panic_propagates(&store).await;
            })
        }

        #[test]
        fn shared_test_async_callback_panic_propagates() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_async_callback_panic_propagates(&store).await;
            })
        }

        #[test]
        fn shared_test_discard_callback_panic_propagates() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_discard_callback_panic_propagates(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_basic() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_basic(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_prefix() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_range() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_range(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_keys_only() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_keys_only(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_closed() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_closed(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_empty_store() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_empty_store(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_start_equals_end() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_start_equals_end(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_prefix_no_match() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_no_match(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_sees_pending_writes() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_sees_pending_writes(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_sees_pending_deletes() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_sees_pending_deletes(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_pending_writes_at_chunk_boundary() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_pending_writes_at_chunk_boundary(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_bounded_scan_across_chunks() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_bounded_scan_across_chunks(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_prefix_scan_across_chunks() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_scan_across_chunks(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_with_prefix() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_with_prefix(&store).await;
            })
        }

        #[test]
        fn shared_test_binary_data() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_binary_data(&store).await;
            })
        }

        #[test]
        fn shared_test_binary_key_ordering() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_binary_key_ordering(&store).await;
            })
        }

        #[test]
        fn shared_test_get_size() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_get_size(&store).await;
            })
        }

        #[test]
        fn shared_test_get_size_with_deletes() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_get_size_with_deletes(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_with_bounds() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_with_bounds(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_single_item_at_start() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_single_item_at_start(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_item_at_end_excluded() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_item_at_end_excluded(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_prefix_between_keys() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_between_keys(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_empty_prefix() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_empty_prefix(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_prefix_with_start() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_prefix_with_start(&store).await;
            })
        }

        #[test]
        fn shared_test_multiple_iterators() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_multiple_iterators(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_seek() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_seek_between_keys() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_between_keys(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_seek_past_end() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_past_end(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reset() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_seek_after_iteration() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_after_iteration(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_seek_reset_on_closed() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_reset_on_closed(&store).await;
            })
        }

        // Reverse iterator edge case tests (from Go corekv)
        #[test]
        fn shared_test_iterator_reverse_start_only() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_start_only(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_end_only() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_end_only(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_end_single_item_out_of_bounds() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_end_single_item_out_of_bounds(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_start_end_no_items_in_range() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_start_end_no_items_in_range(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_seek() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_seek(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_seek_next() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_seek_next(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_end_seek() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_end_seek(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reverse_prefix_next() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reverse_prefix_next(&store).await;
            })
        }

        // Iterator state transition tests (from Go corekv)
        #[test]
        fn shared_test_iterator_reset_partial_iteration() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset_partial_iteration(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reset_after_exhaustion() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset_after_exhaustion(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_reset_then_seek() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_reset_then_seek(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_seek_respects_start_bound() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_seek_respects_start_bound(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_multiple_resets() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_multiple_resets(&store).await;
            })
        }

        // Empty value handling tests
        #[test]
        fn shared_test_empty_value_handling() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_empty_value_handling(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_empty_values() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_empty_values(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_inverted_bounds_yield_empty() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_inverted_bounds_yield_empty(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_inverted_bounds_yield_empty_reverse() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_inverted_bounds_yield_empty_reverse(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_start_beyond_prefix_yields_empty() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_start_beyond_prefix_yields_empty(&store).await;
            })
        }

        #[test]
        fn shared_test_iterator_inverted_bounds_with_pending_writes() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_iterator_inverted_bounds_with_pending_writes(&store).await;
            })
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
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_concurrent_writes_different_keys(store).await;
            })
        }

        #[test]
        fn shared_test_concurrent_writes_same_key() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_concurrent_writes_same_key(store).await;
            })
        }

        #[test]
        fn shared_test_last_writer_wins() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_last_writer_wins(store).await;
            })
        }

        #[test]
        fn shared_test_last_writer_wins_reverse() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_last_writer_wins_reverse(store).await;
            })
        }

        #[test]
        fn shared_test_parallel_stress() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_parallel_stress(store).await;
            })
        }

        #[test]
        fn shared_test_snapshot_isolation_concurrent() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_snapshot_isolation_concurrent(store).await;
            })
        }

        #[test]
        fn shared_test_snapshot_isolation_long_running_reader() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_snapshot_isolation_long_running_reader(store).await;
            })
        }

        #[test]
        fn shared_test_snapshot_isolation_iterator() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_snapshot_isolation_iterator(store).await;
            })
        }

        #[test]
        fn shared_test_write_write_isolation() {
            test_suite::on_workers(|| async {
                let store = $arc_store_fn().await;
                test_suite::test_write_write_isolation(store).await;
            })
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
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_drop_all(&store).await;
            })
        }

        #[test]
        fn shared_test_drop_all_then_write() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_drop_all_then_write(&store).await;
            })
        }

        #[test]
        fn shared_test_drop_all_empty_store() {
            test_suite::on_workers(|| async {
                let store = $store_fn().await;
                test_suite::test_drop_all_empty_store(&store).await;
            })
        }
    };
}

pub use generate_backend_concurrency_tests;
pub use generate_backend_dropable_tests;
pub use generate_backend_tests;
