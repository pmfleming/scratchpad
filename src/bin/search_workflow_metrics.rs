#![forbid(unsafe_code)]
use scratchpad::profile::search_workflow_sample;
use serde_json::{json, Value};

fn main() {
    let repetitions = std::env::var("SCRATCHPAD_WORKFLOW_REPETITIONS").ok()
        .and_then(|value| value.parse::<usize>().ok()).unwrap_or(30).max(1);
    let max_targets = std::env::var("SCRATCHPAD_WORKFLOW_MAX_TARGETS").ok()
        .and_then(|value| value.parse::<usize>().ok()).unwrap_or(10_000).max(1);
    let mut rows = Vec::new();
    for (mode, targets, bytes) in [("active", 1, 1024*1024), ("current", 32, 16384), ("all", 10_000, 16384)] {
        let targets = targets.min(max_targets);
        let samples: Vec<Value> = (0..repetitions).map(|_| search_workflow_sample(mode, targets, bytes)).collect();
        for (operation, budget) in [("dispatch", 50.0), ("first_result", 100.0), ("first_render", 120.0), ("completion", 500.0)] {
            let raw: Vec<f64> = samples.iter().map(|sample| sample[operation].as_f64().expect("measured operation") / 1e6).collect();
            let mut sorted = raw.clone(); sorted.sort_by(f64::total_cmp);
            let median = sorted[sorted.len()/2];
            let maximum = sorted[sorted.len()-1];
            let key = format!("search_workflow_{mode}_{operation}");
            rows.push(json!({"name":format!("{key}/{targets}"), "benchmark_key":key,
                "workload_family":"search", "measurement_status":"valid", "threshold_authoritative":true,
                "threshold_source":"src/bin/search_workflow_metrics.rs", "threshold_ms":budget,
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
    println!("{}", json!({"meta":{"probe_status":"completed", "present_included":false}, "scenarios":rows}));
}
