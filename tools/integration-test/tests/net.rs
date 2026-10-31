#![recursion_limit = "4096"]
#![allow(
    clippy::approx_constant,
    clippy::needless_update,
    clippy::module_inception
)]

mod net {
    mod info {
        mod connect_peers;
        mod disconnect_peers;
    }
    mod one_to_many {
        mod peer {
            mod with_add_update;
        }
        mod replicator {
            mod with_add;
        }
    }
    mod peer_events {
        mod collection;
        mod collection_inactive;
        mod document;
        mod simple;
    }
    mod simple {
        mod peer {
            mod crdt {
                mod lww;
                mod pcounter;
                mod pncounter;
                mod pncounter_convergence;
            }
            mod subscribe {
                mod collection {
                    mod with_add;
                    mod with_add_get;
                    mod with_add_get_remove;
                    mod with_add_remove;
                    mod with_get;
                }
                mod document {
                    mod with_add;
                    mod with_add_get;
                    mod with_add_get_remove;
                    mod with_add_remove;
                    mod with_get;
                }
            }
            mod with_add;
            mod with_add_add_field;
            mod with_delete;
            mod with_update;
            mod with_update_add_field;
            mod with_update_restart;
        }
        mod peer_replicator {
            mod crdt {
                mod pcounter;
                mod pncounter;
            }
            mod with_add;
            mod with_delete;
            mod with_update;
            mod with_update_restart;
        }
        mod replicator {
            mod crdt {
                mod pcounter;
                mod pncounter;
            }
            mod cross_version;
            mod with_add;
            mod with_add_add_field;
            mod with_add_restart;
            mod with_add_update;
            mod with_delete;
            mod with_shared_field;
            mod with_update;
        }
    }
    mod sync {
        mod branchable_collection {
            mod complex_network;
            mod simple;
            mod timeout;
            mod versions;
        }
        mod collection_version {
            mod patch;
            mod simple;
            mod view;
            mod with_nonnillable_array_fields;
            mod with_nonnillable_scalar_fields;
        }
    }
}
