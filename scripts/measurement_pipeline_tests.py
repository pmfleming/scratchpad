import unittest

import capacity_report
import measurement_catalog
import performance_review


MB = 1024 * 1024


def capacity_event(
    scenario: str,
    label: str,
    value: int,
    workload_label: str,
    elapsed_ms: float,
) -> dict:
    return {
        "scenario": scenario,
        "scenario_label": label,
        "workload_family": "capacity-measurement",
        "step_index": 0,
        "workload_value": value,
        "workload_unit": "bytes",
        "workload_label": workload_label,
        "elapsed_ns": int(elapsed_ms * 1_000_000),
        "working_set_bytes": 10 * MB,
        "peak_working_set_bytes": 10 * MB,
        "page_fault_count": 0,
        "handle_count": 1,
        "status": "ok",
    }


class CapacityReportTests(unittest.TestCase):
    def test_text_layout_has_explicit_goal(self) -> None:
        config = capacity_report.SCENARIO_CONFIG["text_layout_ceiling"]

        self.assertEqual(config["threshold_ms"], capacity_report.LARGE_FILE_CAPACITY_THRESHOLD_MS)
        self.assertEqual(config["workload_family"], "text-layout")
        self.assertIn("raw text layout", config["measurement_question"])
        self.assertNotIn("layout_bytes_ceiling", capacity_report.SCENARIO_CONFIG)

    def test_large_file_capacity_scenarios_share_one_threshold(self) -> None:
        self.assertEqual(
            capacity_report.SCENARIO_CONFIG["file_size_ceiling"]["threshold_ms"],
            capacity_report.LARGE_FILE_CAPACITY_THRESHOLD_MS,
        )
        self.assertEqual(
            capacity_report.SCENARIO_CONFIG["text_layout_ceiling"]["threshold_ms"],
            capacity_report.LARGE_FILE_CAPACITY_THRESHOLD_MS,
        )

    def test_legacy_layout_bytes_events_are_reported_as_text_layout(self) -> None:
        payload = capacity_report.summarize_probe(
            [
                capacity_event(
                    "layout_bytes_ceiling",
                    "Layout bytes ceiling sweep",
                    8 * MB,
                    "8.0 MB",
                    120.0,
                ),
                capacity_event(
                    "layout_bytes_ceiling",
                    "Layout bytes ceiling sweep",
                    16 * MB,
                    "16.0 MB",
                    190.0,
                ),
            ]
        )

        row = payload["scenarios"][0]
        self.assertEqual(row["scenario"], "text_layout_ceiling")
        self.assertEqual(row["scenario_label"], "Text Layout")
        self.assertEqual(row["workload_family"], "text-layout")
        self.assertEqual(row["threshold_ms"], 180.0)
        self.assertEqual(row["last_successful_label"], "8.0 MB")
        self.assertEqual(row["first_failure_label"], "16.0 MB")


class PerformanceReviewTests(unittest.TestCase):
    def test_large_files_uses_text_layout_as_layout_boundary_not_gb_file_source(self) -> None:
        large_files = next(
            scenario
            for scenario in performance_review.SCENARIOS
            if scenario["id"] == "large_files"
        )

        self.assertIn("text_layout_ceiling", large_files["capacity_scenarios"])
        self.assertNotIn("layout_bytes_ceiling", large_files["capacity_scenarios"])

        gb_file = next(
            check for check in large_files["scale_checks"] if check["id"] == "gb_file"
        )
        self.assertNotIn("text_layout_ceiling", gb_file["sources"])

        text_layout = next(
            check
            for check in large_files["scale_checks"]
            if check["id"] == "text_layout_batch"
        )
        self.assertEqual(text_layout["minimum"], 8 * MB)
        self.assertEqual(text_layout["sources"], ["text_layout_ceiling"])

    def test_performance_review_accepts_stale_layout_bytes_capacity_rows(self) -> None:
        row = performance_review.normalize_capacity_row(
            {
                "scenario": "layout_bytes_ceiling",
                "scenario_label": "Layout bytes ceiling sweep",
                "workload_family": "capacity-measurement",
            }
        )

        self.assertEqual(row["scenario"], "text_layout_ceiling")
        self.assertEqual(row["scenario_label"], "Text Layout")
        self.assertEqual(row["workload_family"], "text-layout")


class MeasurementCatalogTests(unittest.TestCase):
    def test_performance_review_refresh_regenerates_capacity_before_review(self) -> None:
        tasks = measurement_catalog.build_catalog()["tasks"]
        review = next(task for task in tasks if task["id"] == "performance.report")
        command_names = [command[1] for command in review["commands"]]

        self.assertIn("scripts/capacity_report.py", command_names)
        self.assertLess(
            command_names.index("scripts/capacity_report.py"),
            command_names.index("scripts/performance_review.py"),
        )


if __name__ == "__main__":
    unittest.main()
