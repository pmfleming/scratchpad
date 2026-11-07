//! Probe-side endpoints. This table is compiled with the operation code, not read from the manifest at runtime.
use serde_json::{Value, json};
pub fn resource(id: &str, completed: bool, phases: &Value) -> Value {
    let (endpoint, residency, durability) = match id {
        "large_utf8_load_peak_memory" => ("load_complete", "store_uncontrolled", "not_required"),
        "large_file_first_visible_paint" => {
            ("first_window_prepared", "not_applicable", "not_required")
        }
        "file_backed_chunk_cache_tracking" => {
            ("cache_cycle_complete", "store_uncontrolled", "not_required")
        }
        "file_backed_random_window" => (
            "random_windows_complete",
            "store_uncontrolled",
            "not_required",
        ),
        "file_backed_search" => ("serial_scan_complete", "store_uncontrolled", "not_required"),
        "typing_without_save" => (
            "typing_cycle_complete",
            "store_uncontrolled",
            "not_required",
        ),
        "typing_during_save" => (
            "typing_cycle_complete",
            "store_uncontrolled",
            "not_required",
        ),
        "many_file_resource_tracking" => ("file_cycle_complete", "owned", "not_required"),
        "many_file_lazy_open_tracking" => ("lazy_open_complete", "owned", "not_required"),
        "search_file_size_resource_tracking" => {
            ("search_component_complete", "owned", "not_required")
        }
        "search_target_resource_tracking" => ("target_search_complete", "owned", "not_required"),
        "search_app_result_tracking" => ("results_retained", "owned", "not_required"),
        "edited_buffer_search_preview_rendering" => {
            ("preview_render_complete", "owned", "not_required")
        }
        "paste_allocation" => ("paste_component_complete", "owned", "not_required"),
        "provenance_retained_memory" => ("history_retained", "owned", "not_required"),
        "fragmented_long_session_mutation" => ("mutation_cycle_complete", "owned", "not_required"),
        "tab_count_resource_tracking" => ("tab_cycle_complete", "owned", "not_required"),
        "tab_build_targeted" => ("tabs_constructed", "owned", "not_required"),
        "tab_split_targeted" => ("tabs_split", "owned", "not_required"),
        "tab_combine_targeted" => ("tabs_combined", "owned", "not_required"),
        "tab_strip_frame_rendering" => ("tab_frame_complete", "not_applicable", "not_required"),
        "view_count_resource_tracking" => ("view_cycle_complete", "owned", "not_required"),
        "anchor_heavy_view_editing" => ("anchors_resolved", "owned", "not_required"),
        "session_persist_cost" => ("persist_complete", "store_uncontrolled", "committed_full"),
        "large_dirty_buffer_save" => ("persist_complete", "store_uncontrolled", "committed_full"),
        "session_restore_cost" => (
            "fully_reconstructed_session_retained",
            "store_uncontrolled",
            "not_required",
        ),
        "startup_visible_restore_cost" => (
            "visible_restore_complete",
            "store_uncontrolled",
            "not_required",
        ),
        "session_steady_persist_cost" => (
            "steady_persist_complete",
            "store_uncontrolled",
            "committed_full",
        ),
        "store_import" => (
            "store_import_complete",
            "store_uncontrolled",
            "not_required",
        ),
        "ui_during_store_writes" => (
            "frames_during_writes_complete",
            "store_uncontrolled",
            "not_required",
        ),
        "cold_reads_during_save" => ("cold_reads_complete", "store_cold", "not_required"),
        "aged_store_save" => ("aged_save_complete", "store_uncontrolled", "not_required"),
        "aged_store_restore" => (
            "aged_restore_complete",
            "store_uncontrolled",
            "not_required",
        ),
        "aged_store_close" => ("aged_close_complete", "store_uncontrolled", "not_required"),
        "aged_store_forget" => ("aged_forget_complete", "store_uncontrolled", "not_required"),
        "production_paste" => ("paste_published", "store_uncontrolled", "published_ram"),
        _ => return Value::Null,
    };
    let mut result = json!({"endpoint":endpoint,"residency":residency,
        "durability_at_endpoint":durability,"completed":completed});
    result["publication_observed"] = json!(
        completed
            && phases["publication_endpoint"] == "published_text_and_queued_input_tessellated"
            && phases["queued_typing_preserved"] == true
    );
    result["manifest_commit_full_observed"] =
        json!(completed && phases["transaction_committed"] == true);
    result["previous_full_snapshot_reused"] = json!(
        completed
            && id == "session_steady_persist_cost"
            && phases["transaction_committed"] == false
            && phases["file_write_ns"] == 0
    );
    if id == "cold_reads_during_save"
        && let Some(samples) = phases["cold_read_samples"].as_array()
    {
        // The existing p99 gate uses only physically verified samples.
        // Failed eviction attempts remain in the raw evidence and coverage checks.
        let samples: Vec<_> = samples
            .iter()
            .filter(|s| s["physical_read_verified"] == true)
            .collect();
        result["eviction_succeeded"] =
            json!(!samples.is_empty() && samples.iter().all(|s| s["eviction_succeeded"] == true));
        result["physical_read_bytes"] = json!(
            samples
                .iter()
                .try_fold(0u64, |sum, s| sum.checked_add(s["read_bytes"].as_u64()?))
        );
    }
    result
}

/// Report only quantities actually observed. A body flush or cache occupancy
/// alone cannot establish FULL durability or total unpublished session bytes.
pub fn memory(phases: &Value) -> Value {
    json!({
        "configured_pending_limit_bytes": phases["pending_flow_threshold_bytes"],
        "pending_queue_peak_bytes": phases["peak_pending_bytes"],
        "body_cache_bytes_at_endpoint": phases["cache_bytes"],
        "mutable_tail_bytes_at_endpoint": phases["tail_bytes"],
        "body_flush_after_publication_ns": phases["post_publication_flush_ns"],
        "resident_durable_bytes": null,
        "resident_durable_peak_bytes": null,
        "accepted_not_durable_bytes": null,
        "accepted_not_durable_peak_bytes": null,
        "scratch_peak_bytes": null,
        "publication_timestamp_ns": null,
        "full_commit_timestamp_ns": null
    })
}
pub fn capacity(id: &str, completed: bool) -> Value {
    let (endpoint, residency, durability) = match id {
        "large_file_first_visible_ceiling" => ("first_visible", "not_applicable", "not_required"),
        "large_file_background_index_ceiling" => {
            ("index_complete", "store_uncontrolled", "not_required")
        }
        "in_memory_text_ingest_ceiling" => ("ingest_complete", "owned", "not_required"),
        "text_layout_ceiling" => ("layout_complete", "owned", "not_required"),
        "many_file_first_visible_ceiling" => ("first_visible", "not_applicable", "not_required"),
        "many_file_background_hydration_ceiling" => {
            ("hydration_complete", "store_uncontrolled", "not_required")
        }
        "search_file_size_ceiling" => ("all_ranges_returned", "store_uncontrolled", "not_required"),
        "search_target_count_ceiling" => ("target_search_complete", "owned", "not_required"),
        "tab_count_ceiling" => ("tab_cycle_complete", "owned", "not_required"),
        "split_count_ceiling" => ("split_cycle_complete", "owned", "not_required"),
        "view_count_ceiling" => ("view_cycle_complete", "owned", "not_required"),
        "paste_size_ceiling" => ("paste_published", "store_uncontrolled", "published_ram"),
        _ => return Value::Null,
    };
    // The paste runner returns only after its publication assertions succeed.
    json!({"endpoint":endpoint,"residency":residency,"durability_at_endpoint":durability,
        "completed":completed,"publication_observed":completed && id == "paste_size_ceiling"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_observations_use_the_verified_statistic_population() {
        let phases = json!({"cold_read_samples":[
            {"physical_read_verified":false,"eviction_succeeded":false,"read_bytes":null},
            {"physical_read_verified":true,"eviction_succeeded":true,"read_bytes":4096}
        ]});
        let observed = resource("cold_reads_during_save", true, &phases);
        assert_eq!(observed["eviction_succeeded"], true);
        assert_eq!(observed["physical_read_bytes"], 4096);
        let absent = resource(
            "cold_reads_during_save",
            true,
            &json!({"cold_read_samples":[]}),
        );
        assert_eq!(absent["eviction_succeeded"], false);
    }

    #[test]
    fn body_flush_does_not_claim_full_durability_or_total_exposure() {
        let observed = memory(
            &json!({"post_publication_flush_ns":42,"peak_pending_bytes":123,
            "pending_flow_threshold_bytes":33554432,"cache_bytes":456}),
        );
        assert_eq!(observed["pending_queue_peak_bytes"], 123);
        assert_eq!(observed["configured_pending_limit_bytes"], 33554432);
        assert!(observed["full_commit_timestamp_ns"].is_null());
        assert!(observed["accepted_not_durable_peak_bytes"].is_null());
        assert!(observed["resident_durable_bytes"].is_null());
    }

    #[test]
    fn probe_endpoints_cover_the_versioned_manifest() {
        let manifest: Value =
            serde_json::from_str(include_str!("../../benches/measurement_manifest.json")).unwrap();
        for section in ["resource_budgets", "capacity_budgets"] {
            for (id, entry) in manifest[section].as_object().unwrap() {
                let observed = if section == "resource_budgets" {
                    resource(id, true, &json!({}))
                } else {
                    capacity(id, true)
                };
                for key in ["endpoint", "residency", "durability_at_endpoint"] {
                    assert_eq!(observed[key], entry["outcome"][key], "{section}/{id}/{key}");
                }
            }
        }
    }
}
