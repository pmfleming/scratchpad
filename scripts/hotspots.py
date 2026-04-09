import argparse
import json
import subprocess
import sys
import os
import base64
import io
import shutil
from datetime import datetime
from dataclasses import dataclass, asdict
from typing import List, Dict

import pandas as pd
import numpy as np
import matplotlib
import matplotlib.pyplot as plt
import squarify
from jinja2 import Template

# Ensure plots can be generated without a GUI
matplotlib.use('Agg')

@dataclass
class CodeMetrics:
    name: str
    kind: str
    start_line: int
    end_line: int
    cognitive: float
    cyclomatic: float
    mi: float
    effort: float
    sloc: float
    ploc: float
    cloc: float
    bugs: float
    abc_mag: float
    nom_fn: int
    nom_cl: int
    score: float = 0.0
    signals: str = ""
    abc_density: float = 0.0
    complexity_density: float = 0.0

    @property
    def short_name(self) -> str:
        if self.kind == "unit":
            return os.path.basename(self.name)
        if "::" in self.name:
            return self.name.split("::")[-1]
        return os.path.basename(self.name) if os.sep in self.name else self.name


class HotspotAnalyzer:
    def __init__(self, top=15, scope="all", include_anonymous=False):
        self.top = top
        self.scope = scope
        self.include_anonymous = include_anonymous

    def _extract_metric(self, metrics: Dict, group: str, name: str, default: float = 0.0) -> float:
        return float(metrics.get(group, {}).get(name, default))

    def process_node(self, node: Dict) -> CodeMetrics:
        m = node.get("metrics", {})
        
        metrics = CodeMetrics(
            name=node.get("name", "unknown"),
            kind=node.get("kind", "unknown"),
            start_line=node.get("start_line", 0),
            end_line=node.get("end_line", 0),
            cognitive=self._extract_metric(m, "cognitive", "sum"),
            cyclomatic=self._extract_metric(m, "cyclomatic", "sum"),
            mi=self._extract_metric(m, "mi", "mi_visual_studio", 100.0),
            effort=self._extract_metric(m, "halstead", "effort"),
            sloc=self._extract_metric(m, "loc", "sloc"),
            ploc=self._extract_metric(m, "loc", "ploc"),
            cloc=self._extract_metric(m, "loc", "cloc"),
            bugs=self._extract_metric(m, "halstead", "bugs"),
            abc_mag=self._extract_metric(m, "abc", "magnitude"),
            nom_fn=int(self._extract_metric(m, "nom", "functions")),
            nom_cl=int(self._extract_metric(m, "nom", "closures")),
        )
        
        metrics.score = self.calculate_score(metrics)
        metrics.signals = self.generate_signals(metrics)
        
        if metrics.sloc > 0:
            metrics.abc_density = metrics.abc_mag / metrics.sloc
            metrics.complexity_density = metrics.score / metrics.sloc
            
        return metrics

    def calculate_score(self, m: CodeMetrics) -> float:
        score = (m.cognitive * 4.0) + (m.cyclomatic * 2.5)
        score += max(0.0, 70.0 - m.mi) * 1.5
        score += min(30.0, m.effort / 1000.0)
        score += min(20.0, m.sloc / 10.0)
        return round(score, 2)

    def generate_signals(self, m: CodeMetrics) -> str:
        sigs = []
        if m.cognitive >= 8: sigs.append(f"high cognitive={m.cognitive}")
        if m.cyclomatic >= 12: sigs.append(f"high cyclomatic={m.cyclomatic}")
        if m.mi < 40: sigs.append(f"low MI={m.mi:.1f}")
        if m.effort >= 15000: sigs.append(f"high effort={m.effort:,.0f}")
        if m.sloc >= 150: sigs.append(f"large sloc={m.sloc}")
        return ", ".join(sigs) if sigs else "stable"

    def flatten(self, node: Dict, results: List[CodeMetrics]):
        results.append(self.process_node(node))
        for child in node.get("spaces", []):
            self.flatten(child, results)

    def run(self, paths: List[str]) -> List[CodeMetrics]:
        if not shutil.which("rust-code-analysis-cli"):
            print("Error: 'rust-code-analysis-cli' not found in PATH.")
            sys.exit(1)

        cmd = ["rust-code-analysis-cli", "--metrics", "--paths"] + paths + ["--output-format", "json"]
        try:
            res = subprocess.run(cmd, capture_output=True, text=True, check=True)
            all_data = []
            
            for line in res.stdout.splitlines():
                if not line.strip(): continue
                clean_line = line.replace('"N1":', '"halstead_N1":').replace('"N2":', '"halstead_N2":')
                try:
                    parsed_json = json.loads(clean_line)
                    self.flatten(parsed_json, all_data)
                except json.JSONDecodeError:
                    print(f"Warning: Skipping malformed CLI output line: {clean_line[:50]}...", file=sys.stderr)
                    continue

            scope_map = {"files": ["unit"], "functions": ["function"]}
            target_kinds = scope_map.get(self.scope, ["unit", "function"])
            
            filtered = [m for m in all_data if m.kind in target_kinds]
            if not self.include_anonymous:
                filtered = [m for m in filtered if m.name != "<anonymous>"]

            # Return ALL for visualization purposes (slicing happens later or in CLI mode)
            return sorted(filtered, key=lambda x: (-x.score, x.name))
            
        except Exception as e:
            print(f"Analysis failed: {e}")
            sys.exit(1)


def get_plot_base64():
    buf = io.BytesIO()
    plt.savefig(buf, format='png', bbox_inches='tight', dpi=120, transparent=True)
    plt.close()
    return base64.b64encode(buf.getvalue()).decode('utf-8')


def generate_visual_report(all_ranked: List[CodeMetrics], paths: List[str], top_n: int):
    df = pd.DataFrame([asdict(m) for m in all_ranked])
    # Add calculated columns to main DF before slicing to avoid KeyErrors later
    df['short_name'] = df['name'].apply(lambda n: next((m.short_name for m in all_ranked if m.name == n), os.path.basename(n)))
    df['inv_mi'] = 100.0 - df['mi']
    
    # Isolate top items for ranked charts
    top_df = df.head(top_n)
    
    plt.style.use('dark_background')
    
    # 1. Scatter: Complexity Map
    num_points = len(df)
    point_alpha = max(0.4, min(0.8, 150 / (num_points + 1))) 
    plt.figure(figsize=(12, 6))
    scatter = plt.scatter(
        df['sloc'], df['cognitive'], c=df['score'], cmap='inferno', 
        s=df['cyclomatic']*15 + 20, alpha=point_alpha, edgecolors='white', linewidths=0.5
    )
    plt.colorbar(scatter, label='Hotspot Score')
    plt.title('Complexity Landscape (Bubble Size=Cyclomatic)', pad=20, color='#ce9178')
    plt.xlabel('Lines of Code (SLOC)')
    plt.ylabel('Cognitive Complexity')
    plt.grid(True, linestyle='--', alpha=0.3)
    chart_scatter = get_plot_base64()

    # 2. Bar: Top Hotspots
    plt.figure(figsize=(10, 5))
    top_10 = top_df.head(10)
    plt.barh(top_10['short_name'], top_10['score'], color='#569cd6')
    plt.gca().invert_yaxis()
    plt.title('Top Hotspot Scores', color='#ce9178')
    plt.xlabel('Score')
    plt.grid(axis='x', linestyle='--', alpha=0.3)
    chart_bar = get_plot_base64()

    # 3. Treemap: Codebase Heatmap
    plt.figure(figsize=(12, 6))
    tm_df = df[df['sloc'] > 0].copy()
    norm = matplotlib.colors.Normalize(vmin=tm_df['score'].min(), vmax=tm_df['score'].max())
    colors = [matplotlib.cm.inferno(norm(value)) for value in tm_df['score']]
    # Only label the largest 20% to avoid extreme text overlap
    sloc_threshold = tm_df['sloc'].quantile(0.8)
    labels = [row['short_name'] if row['sloc'] > sloc_threshold else "" for i, row in tm_df.iterrows()]
    
    squarify.plot(sizes=tm_df['sloc'], label=labels, color=colors, alpha=0.8, 
                  text_kwargs={'color':'white', 'fontsize':8, 'weight':'bold'})
    plt.title('Codebase Heatmap (Size=SLOC, Color=Score)', pad=20, color='#ce9178')
    plt.axis('off')
    chart_treemap = get_plot_base64()

    # 4. Radar: Hotspot Profiling (Top 3)
    metrics = ['cognitive', 'cyclomatic', 'effort', 'sloc', 'inv_mi']
    radar_labels = ['Cognitive', 'Cyclomatic', 'Effort', 'SLOC', 'Maint. Drop']
    
    top_3 = top_df.head(3)
    normalized = pd.DataFrame()
    for m in metrics:
        max_val = df[m].max()
        normalized[m] = top_3[m] / max_val if max_val > 0 else 0

    angles = np.linspace(0, 2 * np.pi, len(metrics), endpoint=False).tolist()
    angles += angles[:1]

    fig, ax = plt.subplots(figsize=(8, 6), subplot_kw=dict(polar=True))
    fig.patch.set_alpha(0.0)
    ax.set_facecolor('#1e1e1e')
    
    colors = ['#f44747', '#d7ba7d', '#569cd6']
    for idx, (i, row) in enumerate(top_3.iterrows()):
        values = normalized.loc[i].tolist()
        values += values[:1]
        color = colors[idx % len(colors)]
        ax.plot(angles, values, linewidth=2, label=row['short_name'], color=color)
        ax.fill(angles, values, alpha=0.15, color=color)

    ax.set_yticklabels([])
    ax.set_xticks(angles[:-1])
    ax.set_xticklabels(radar_labels, color='#d4d4d4', size=10)
    ax.spines['polar'].set_color('#3e3e3e')
    plt.title('Hotspot Profiles (Relative to Max Limits)', color='#ce9178', pad=20)
    plt.legend(loc='upper right', bbox_to_anchor=(1.3, 1.1), facecolor='#252526', edgecolor='#3e3e3e')
    chart_radar = get_plot_base64()

    # 5. Histogram: Overall Health Distribution
    plt.figure(figsize=(10, 5))
    bins = max(10, min(40, len(df)//3))
    plt.hist(df['cognitive'], bins=bins, color='#c586c0', edgecolor='#1e1e1e')
    plt.title('Codebase Health: Cognitive Complexity Distribution', color='#ce9178')
    plt.xlabel('Cognitive Complexity')
    plt.ylabel('Number of Entities')
    plt.grid(axis='y', linestyle='--', alpha=0.3)
    chart_hist = get_plot_base64()

    template_str = """
<!DOCTYPE html>
<html>
<head>
    <title>Code Hotspots Report</title>
    <style>
        body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background: #1e1e1e; color: #d4d4d4; margin: 20px; }
        .container { max-width: 1400px; margin: auto; }
        h1 { color: #569cd6; border-bottom: 1px solid #3e3e3e; padding-bottom: 10px; margin-bottom: 5px; }
        .summary { margin-bottom: 30px; color: #888; font-size: 0.9em; }
        .charts { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; margin-bottom: 40px; }
        .chart-box { background: #252526; border: 1px solid #3e3e3e; padding: 15px; border-radius: 4px; box-shadow: 0 4px 6px rgba(0,0,0,0.3); }
        .chart-full { grid-column: span 2; }
        img { max-width: 100%; height: auto; display: block; margin: auto; }
        
        .table-container { overflow-x: auto; background: #252526; border-radius: 4px; border: 1px solid #3e3e3e; margin-top: 40px; }
        table { width: 100%; border-collapse: collapse; }
        th, td { text-align: left; padding: 12px; border-bottom: 1px solid #3e3e3e; font-size: 0.85em; }
        th { background: #333; color: #ce9178; position: sticky; top: 0; }
        tr:hover { background: #2d2d2d; }
        
        .score-high { color: #f44747; font-weight: bold; }
        .score-med { color: #d7ba7d; }
        .score-low { color: #b5cea8; }
        .signal-tag { font-size: 0.8em; padding: 2px 6px; border-radius: 3px; background: #3e3e42; color: #d4d4d4; }
        .badge { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 0.75em; background: #0e639c; color: white; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Code Hotspots Analysis</h1>
        <div class="summary">
            Generated on: {{ date }} | Paths: {{ paths | join(', ') }}
        </div>

        <div class="charts">
            <div class="chart-box chart-full">
                <img src="data:image/png;base64,{{ chart_treemap }}" alt="Codebase Heatmap">
            </div>
            <div class="chart-box chart-full">
                <img src="data:image/png;base64,{{ chart_scatter }}" alt="Complexity Map">
            </div>
            
            <div class="chart-box">
                <img src="data:image/png;base64,{{ chart_bar }}" alt="Top Scores">
            </div>
            <div class="chart-box">
                <img src="data:image/png;base64,{{ chart_radar }}" alt="Hotspot Profiles">
            </div>
            <div class="chart-box chart-full">
                <img src="data:image/png;base64,{{ chart_hist }}" alt="Cognitive Distribution">
            </div>
        </div>

        <div class="table-container">
            <table>
                <thead>
                    <tr>
                        <th>Rank</th>
                        <th>Location</th>
                        <th>Score</th>
                        <th>Dens.</th>
                        <th>MI</th>
                        <th>Cog</th>
                        <th>Cyc</th>
                        <th>ABC-D</th>
                        <th>Bugs</th>
                        <th>SLOC</th>
                        <th>Signals</th>
                    </tr>
                </thead>
                <tbody>
                    {% for m in top_ranked %}
                    <tr>
                        <td>{{ loop.index }}</td>
                        <td title="{{ m.name }}">
                            <span class="badge">{{ m.kind }}</span> 
                            <strong>{{ m.short_name }}</strong>:{{ m.start_line }}
                        </td>
                        <td class="{{ 'score-high' if m.score > 300 else 'score-med' if m.score > 150 else 'score-low' }}">
                            {{ "%.2f"|format(m.score) }}
                        </td>
                        <td>{{ "%.2f"|format(m.complexity_density) }}</td>
                        <td>{{ "%.1f"|format(m.mi) }}</td>
                        <td>{{ m.cognitive | int }}</td>
                        <td>{{ m.cyclomatic | int }}</td>
                        <td>{{ "%.3f"|format(m.abc_density) }}</td>
                        <td>{{ "%.3f"|format(m.bugs) }}</td>
                        <td>{{ m.sloc | int }}</td>
                        <td>
                            {% for sig in m.signals.split(', ') %}
                                {% if sig %}
                                    <span class="signal-tag">{{ sig }}</span>
                                {% endif %}
                            {% endfor %}
                        </td>
                    </tr>
                    {% endfor %}
                </tbody>
            </table>
        </div>
    </div>
</body>
</html>
    """
    
    template = Template(template_str)
    html = template.render(
        date=datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
        paths=paths,
        top_ranked=top_df.to_dict('records'),
        chart_scatter=chart_scatter,
        chart_bar=chart_bar,
        chart_treemap=chart_treemap,
        chart_radar=chart_radar,
        chart_hist=chart_hist
    )
    
    output_file = "hotspots.html"
    with open(output_file, "w", encoding="utf-8") as f:
        f.write(html)
    print(f"Visual report saved to: {os.path.abspath(output_file)}")


def main():
    parser = argparse.ArgumentParser(description="Rust Code Hotspot Analyzer")
    parser.add_argument("--paths", nargs="+", default=["src"], help="Paths to analyze")
    parser.add_argument("--top", type=int, default=15, help="Number of top hotspots to show")
    parser.add_argument("--scope", choices=["all", "files", "functions"], default="all", help="Scope of analysis")
    parser.add_argument("--include-anonymous", action="store_true", help="Include anonymous functions")
    parser.add_argument("--mode", choices=["hotspot", "review", "visual", "json"], default="hotspot", help="Output mode")
    
    args = parser.parse_args()
    
    analyzer = HotspotAnalyzer(top=args.top, scope=args.scope, include_anonymous=args.include_anonymous)
    # In visual mode or json mode, we want the whole dataset.
    all_ranked = analyzer.run(args.paths)
    
    if args.mode == "json":
        print(json.dumps([asdict(m) for m in all_ranked]))
        return

    if args.mode == "hotspot":
        print(f"Hotspots Summary\nPaths: {', '.join(args.paths)}\nScope: {args.scope}\n")
        for i, m in enumerate(all_ranked[:args.top]):
            print(f"{i+1:2}. [{m.kind}] {m.name}:{m.start_line}")
            print(f"    score={m.score} | cog={m.cognitive:.0f} | cyc={m.cyclomatic:.0f} | MI={m.mi:.1f} | sloc={m.sloc:.0f} | bugs={m.bugs:.3f}")
            print(f"    signals: {m.signals}")
            
    elif args.mode == "review":
        print("FULL REVIEW: Complexity Hotspots")
        print("=" * 40 + "\n")
        for i, m in enumerate(all_ranked[:args.top]):
            print(f"{i+1}. [{m.kind}] {m.name}:{m.start_line}")
            print(f"   Summary: Score={m.score}, Density={m.complexity_density:.2f}, MI={m.mi:.1f}, SLOC={m.sloc:.0f}")
            print("   Logic Complexity:")
            print(f"     Cognitive:  {m.cognitive:>4.0f} (nested flow)")
            print(f"     Cyclomatic: {m.cyclomatic:>4.0f} (paths)")
            print(f"     ABC Mag:    {m.abc_mag:>4.1f} (Density: {m.abc_density:.3f})")
            print("   Maintainability & Effort:")
            print(f"     MI VS:      {m.mi:>4.1f}")
            print(f"     Halstead:   {m.effort:>4,.0f} effort, {m.bugs:.3f} bugs")
            print("   Structure:")
            print(f"     Lines:      {m.sloc:.0f} sloc, {m.ploc:.0f} ploc, {m.cloc:.0f} cloc")
            if m.kind == "unit":
                print(f"     Content:    {m.nom_fn} functions, {m.nom_cl} closures")
            print(f"   SIGNALS: {m.signals}\n")

    elif args.mode == "visual":
        generate_visual_report(all_ranked, args.paths, args.top)

if __name__ == "__main__":
    main()