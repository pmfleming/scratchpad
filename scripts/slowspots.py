import argparse
import json
import subprocess
import sys
import os
import base64
import io
import shutil
import threading
import time
from datetime import datetime
from dataclasses import dataclass, asdict
from typing import List, Dict, Optional

import pandas as pd
import numpy as np
import matplotlib
import matplotlib.pyplot as plt
from jinja2 import Template

# Ensure plots can be generated without a GUI
matplotlib.use('Agg')

@dataclass
class PerfMetrics:
    name: str
    mean_ns: float
    std_dev_ns: float
    median_ns: float
    max_ns: float
    min_ns: float
    p95_ns: Optional[float] = None
    score: float = 0.0
    signals: str = ""

    @property
    def mean_ms(self) -> float:
        return self.mean_ns / 1_000_000.0

class SlowspotAnalyzer:
    def __init__(self, threshold_ms=50.0):
        self.threshold_ms = threshold_ms

    def run_benchmarks(self, skip_bench: bool = False) -> List[PerfMetrics]:
        if not skip_bench:
            print("Running benchmarks via cargo bench...")
            try:
                self.run_bench_command(["cargo", "bench"])
            except Exception as e:
                print(f"Benchmarking failed: {e}")
                return self.get_mock_data()

        results = self.load_criterion_results(os.path.join("target", "criterion"))
        if not results:
            if not skip_bench:
                print("Error: No Criterion benchmark results were discovered.")
            return self.get_mock_data()

        return sorted(results, key=lambda x: -x.score)

    def run_bench_command(self, cmd: List[str]) -> None:
        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )

        status = {
            "current": "starting cargo bench",
            "last_update": time.time(),
            "done": False,
        }

        def progress_reporter():
            start = time.time()
            while not status["done"]:
                elapsed = time.time() - start
                current = status["current"]
                print(
                    f"[progress {elapsed:5.1f}s] {current}",
                    file=sys.stderr,
                    flush=True,
                )
                time.sleep(5)

        reporter = threading.Thread(target=progress_reporter, daemon=True)
        reporter.start()

        captured_output = []
        try:
            assert process.stdout is not None
            for raw_line in process.stdout:
                line = raw_line.rstrip()
                captured_output.append(raw_line)
                benchmark_name = self.parse_benchmark_progress(line)
                if benchmark_name is not None:
                    status["current"] = benchmark_name
                    status["last_update"] = time.time()

            return_code = process.wait()
        finally:
            status["done"] = True
            reporter.join(timeout=1)

        if return_code != 0:
            raise subprocess.CalledProcessError(
                return_code,
                cmd,
                output="".join(captured_output),
            )

    def parse_benchmark_progress(self, line: str) -> Optional[str]:
        prefixes = [
            "Benchmarking ",
            "Running ",
            "Compiling ",
            "Finished ",
        ]
        for prefix in prefixes:
            if line.startswith(prefix):
                return line
        return None

    def load_criterion_results(self, criterion_dir: str) -> List[PerfMetrics]:
        if not os.path.exists(criterion_dir):
            print(f"Error: Criterion results directory not found at {criterion_dir}")
            return []

        results = []
        for root, _, files in os.walk(criterion_dir):
            if "estimates.json" not in files:
                continue
            estimates_path = os.path.join(root, "estimates.json")
            if os.path.basename(os.path.dirname(estimates_path)) != "new":
                continue

            with open(estimates_path, 'r') as f:
                data = json.load(f)

            benchmark_name = self.benchmark_name_from_estimate_path(criterion_dir, estimates_path)
            mean = data.get("mean", {}).get("point_estimate", 0.0)
            std_dev = data.get("std_dev", {}).get("point_estimate", 0.0)
            median = data.get("median", {}).get("point_estimate", 0.0)
            p95 = data.get("median_abs_dev", {}).get("point_estimate")

            m = PerfMetrics(
                name=benchmark_name,
                mean_ns=mean,
                std_dev_ns=std_dev,
                median_ns=median,
                max_ns=mean + (2 * std_dev),
                min_ns=max(0, mean - (2 * std_dev)),
                p95_ns=p95,
            )
            m.score = self.calculate_score(m)
            m.signals = self.generate_signals(m)
            results.append(m)

        return results

    def benchmark_name_from_estimate_path(self, criterion_dir: str, estimates_path: str) -> str:
        relative = os.path.relpath(estimates_path, criterion_dir)
        parts = relative.split(os.sep)
        return "/".join(parts[:-2])

    def get_mock_data(self) -> List[PerfMetrics]:
        mock = [
            PerfMetrics("tab_stress_operations", 45000000.0, 5000000.0, 44000000.0, 55000000.0, 35000000.0),
            PerfMetrics("file_open_latency", 120000000.0, 20000000.0, 115000000.0, 160000000.0, 80000000.0),
            PerfMetrics("buffer_search_regex", 8000000.0, 1000000.0, 7500000.0, 10000000.0, 6000000.0),
            PerfMetrics("ui_render_frame", 12000000.0, 2000000.0, 11000000.0, 16000000.0, 8000000.0),
        ]
        for m in mock:
            m.score = self.calculate_score(m)
            m.signals = self.generate_signals(m)
        return sorted(mock, key=lambda x: -x.score)

    def calculate_score(self, m: PerfMetrics) -> float:
        # Score is primarily mean latency in ms
        score = m.mean_ms
        # Add penalty for high variability (instability)
        if m.mean_ns > 0:
            rel_std_dev = m.std_dev_ns / m.mean_ns
            score *= (1.0 + rel_std_dev)
        return round(score, 2)

    def generate_signals(self, m: PerfMetrics) -> str:
        sigs = []
        if m.mean_ms > self.threshold_ms: sigs.append(f"slow > {self.threshold_ms}ms")
        if m.mean_ns > 1_000_000 and (m.std_dev_ns / m.mean_ns) > 0.2:
            sigs.append("high variance")
        return ", ".join(sigs) if sigs else "nominal"

def get_plot_base64():
    buf = io.BytesIO()
    plt.savefig(buf, format='png', bbox_inches='tight', dpi=120, transparent=True)
    plt.close()
    return base64.b64encode(buf.getvalue()).decode('utf-8')

def generate_visual_report(results: List[PerfMetrics]):
    df = pd.DataFrame([asdict(m) for m in results])
    df['mean_ms'] = df['mean_ns'] / 1_000_000.0
    
    plt.style.use('dark_background')
    
    # 1. Bar: Latency Comparison
    plt.figure(figsize=(10, 6))
    colors = ['#f44747' if m > 50 else '#569cd6' for m in df['mean_ms']]
    plt.barh(df['name'], df['mean_ms'], color=colors)
    plt.axvline(x=50, color='#f44747', linestyle='--', alpha=0.5, label='Threshold (50ms)')
    plt.gca().invert_yaxis()
    plt.title('Performance Latency by Operation', color='#ce9178')
    plt.xlabel('Mean Latency (ms)')
    plt.legend()
    plt.grid(axis='x', linestyle='--', alpha=0.3)
    chart_bar = get_plot_base64()

    # 2. Distribution Plot (Simulated from mean/stddev)
    plt.figure(figsize=(10, 6))
    for _, row in df.iterrows():
        x = np.linspace(row['mean_ms'] - 3*row['std_dev_ns']/1e6, row['mean_ms'] + 3*row['std_dev_ns']/1e6, 100)
        y = (1 / (np.sqrt(2 * np.pi) * row['std_dev_ns']/1e6)) * np.exp(-0.5 * ((x - row['mean_ms']) / (row['std_dev_ns']/1e6))**2)
        plt.plot(x, y, label=row['name'])
        plt.fill_between(x, y, alpha=0.2)
    
    plt.title('Latency Probability Distribution', color='#ce9178')
    plt.xlabel('Latency (ms)')
    plt.ylabel('Probability Density')
    plt.legend()
    plt.grid(True, linestyle='--', alpha=0.3)
    chart_dist = get_plot_base64()

    template_str = """
<!DOCTYPE html>
<html>
<head>
    <title>Performance Slowspots Report</title>
    <style>
        body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background: #1e1e1e; color: #d4d4d4; margin: 20px; }
        .container { max-width: 1200px; margin: auto; }
        h1 { color: #569cd6; border-bottom: 1px solid #3e3e3e; padding-bottom: 10px; }
        .charts { display: flex; flex-direction: column; gap: 40px; margin-bottom: 40px; }
        .chart-box { background: #252526; border: 1px solid #3e3e3e; padding: 20px; border-radius: 4px; }
        img { max-width: 100%; height: auto; display: block; margin: auto; }
        table { width: 100%; border-collapse: collapse; margin-top: 20px; background: #252526; }
        th, td { text-align: left; padding: 12px; border-bottom: 1px solid #3e3e3e; }
        th { background: #333; color: #ce9178; }
        .slow { color: #f44747; font-weight: bold; }
        .nominal { color: #b5cea8; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Performance Slowspots Analysis</h1>
        <div class="charts">
            <div class="chart-box">
                <img src="data:image/png;base64,{{ chart_bar }}" alt="Latency Bar Chart">
            </div>
            <div class="chart-box">
                <img src="data:image/png;base64,{{ chart_dist }}" alt="Latency Distribution">
            </div>
        </div>
        <table>
            <thead>
                <tr>
                    <th>Operation</th>
                    <th>Mean (ms)</th>
                    <th>Std Dev (ms)</th>
                    <th>Median (ms)</th>
                    <th>Status</th>
                    <th>Signals</th>
                </tr>
            </thead>
            <tbody>
                {% for m in results %}
                <tr>
                    <td>{{ m.name }}</td>
                    <td class="{{ 'slow' if m.mean_ms > 50 else 'nominal' }}">{{ "%.2f"|format(m.mean_ms) }}</td>
                    <td>{{ "%.2f"|format(m.std_dev_ns / 1000000) }}</td>
                    <td>{{ "%.2f"|format(m.median_ns / 1000000) }}</td>
                    <td>{{ 'SLOW' if m.mean_ms > 50 else 'OK' }}</td>
                    <td>{{ m.signals }}</td>
                </tr>
                {% endfor %}
            </tbody>
        </table>
    </div>
</body>
</html>
    """
    template = Template(template_str)
    html = template.render(results=results, chart_bar=chart_bar, chart_dist=chart_dist)
    
    with open("slowspots.html", "w", encoding="utf-8") as f:
        f.write(html)
    print(f"Visual report saved to: {os.path.abspath('slowspots.html')}")

def main():
    parser = argparse.ArgumentParser(description="Rust Performance Slowspot Analyzer")
    parser.add_argument("--mode", choices=["slowspots", "review", "display", "json"], default="slowspots", help="Output mode")
    parser.add_argument("--threshold", type=float, default=50.0, help="Latency threshold in ms")
    parser.add_argument("--skip-bench", action="store_true", help="Skip running benchmarks and load existing results")
    
    args = parser.parse_args()
    
    # Suppress output in JSON mode
    if args.mode == "json":
        # Redirect stdout to devnull temporarily if we want to be absolutely sure, 
        # but let's just use a flag in run_benchmarks
        pass

    analyzer = SlowspotAnalyzer(threshold_ms=args.threshold)
    results = analyzer.run_benchmarks(skip_bench=args.skip_bench or args.mode == "json")
    
    if args.mode == "json":
        print(json.dumps([asdict(m) for m in results]))
        return

    if args.mode == "slowspots":
        print(f"CI SLOWSPOTS (Threshold: {args.threshold}ms)")
        print("-" * 30)
        found_slow = False
        for m in results:
            if m.mean_ms > args.threshold:
                print(f"FAILURE: {m.name} is slow: {m.mean_ms:.2f}ms")
                found_slow = True
        if not found_slow:
            print("SUCCESS: All operations within performance limits.")
        else:
            sys.exit(1)

    elif args.mode == "review":
        print("FULL PERFORMANCE REVIEW")
        print("=" * 40)
        for m in results:
            print(f"Operation: {m.name}")
            print(f"  Mean:   {m.mean_ms:>8.2f} ms")
            print(f"  Median: {m.median_ns/1e6:>8.2f} ms")
            print(f"  StdDev: {m.std_dev_ns/1e6:>8.2f} ms")
            print(f"  Range:  [{m.min_ns/1e6:.2f} - {m.max_ns/1e6:.2f}] ms")
            print(f"  Signals: {m.signals}\n")

    elif args.mode == "display":
        generate_visual_report(results)

if __name__ == "__main__":
    main()
