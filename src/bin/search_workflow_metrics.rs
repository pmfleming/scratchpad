#![forbid(unsafe_code)]
use scratchpad::profile::search_workflow_sample;
use serde_json::{Value, json};

fn main() {
    let manifest: Value =
        serde_json::from_str(include_str!("../../benches/measurement_manifest.json"))
            .expect("valid project measurement manifest");
    let repetitions = std::env::var("SCRATCHPAD_WORKFLOW_REPETITIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(30)
        .max(1);
    let max_targets = std::env::var("SCRATCHPAD_WORKFLOW_MAX_TARGETS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
        .max(1);
    let mut rows = Vec::new();
    for mode in ["active", "current", "all"] {
        let workload = &manifest["workflow_workloads"][mode];
        let size = |key: &str| {
            workload[key]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0)
                .expect("positive workflow fixture size")
        };
        let targets = size("targets").min(max_targets);
        let bytes = size("bytes_per_target");
        let samples: Vec<Value> = (0..repetitions)
            .map(|_| search_workflow_sample(mode, targets, bytes))
            .collect();
        for operation in ["dispatch", "first_result", "first_render", "completion"] {
            let key = format!("search_workflow_{mode}_{operation}");
            let budget = manifest["probe_measurements"][&key]["threshold_ms"]
                .as_f64()
                .filter(|value| value.is_finite() && *value > 0.0)
                .expect("positive project-owned workflow budget");
            let raw: Vec<f64> = samples
                .iter()
                .map(|sample| sample[operation].as_f64().expect("measured operation") / 1e6)
                .collect();
            let mut sorted = raw.clone();
            sorted.sort_by(f64::total_cmp);
            let median = (sorted[sorted.len() / 2] + sorted[(sorted.len() - 1) / 2]) / 2.0;
            let maximum = sorted[sorted.len() - 1];
            rows.push(json!({"name":format!("{key}/{targets}"), "benchmark_key":key,
                "workload_family":"search", "measurement_status":"valid", "threshold_authoritative":true,
                "threshold_source":"benches/measurement_manifest.json", "threshold_ms":budget,
                "measurement_scope":format!("query_command_to_{operation}"),
                "measurement_role":"promise_health", "present_included":false,
                "omitted_work":["GPU upload and submission", "compositor and display present"],
                "parameter_value":targets, "parameter_unit":"targets", "bytes_per_target":bytes,
                "total_bytes":targets*bytes, "mode":mode, "latency_kind":operation,
                "repetition_count":repetitions, "independent_fixtures":true, "cache_state":"warm_in_memory",
                "sample_elapsed_ms":raw, "raw_samples":samples,
                "mean_ms":sorted.iter().sum::<f64>()/sorted.len() as f64, "median_ms":median, "max_ms":maximum,
                "budget_probe_ns":median*1e6, "budget_probe_label":"median_of_independent_trials",
                "budget_status":if median > budget {"fail"} else if repetitions >= 30 && maximum <= budget {"pass"} else {"uncertain"},
                "statistical_claim":"observed repeated-run budget compliance; no population tail guarantee"}));
        }
    }
    println!(
        "{}",
        json!({"meta":{"probe_status":"completed", "present_included":false}, "scenarios":rows})
    );
}
