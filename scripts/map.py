import os
import re
import json
import subprocess
import argparse
from typing import Dict, List, Set, Tuple

class ArchitectureMapper:
    def __init__(self):
        self.dependencies: Dict[str, Set[str]] = {}
        self.metrics: Dict[str, Dict] = {}
        self.performance: Dict[str, Dict] = {}
        self.file_to_mod: Dict[str, str] = {}

    def extract_dependencies(self, root_dir: str):
        """Extract internal module dependencies and build the module tree."""
        for root, _, files in os.walk(root_dir):
            for file in files:
                if file.endswith(".rs"):
                    file_path = os.path.join(root, file)
                    rel_path = os.path.relpath(file_path, root_dir)
                    
                    # Normalize module name
                    mod_name = rel_path.replace(os.sep, "::").replace(".rs", "")
                    if mod_name.endswith("::mod"):
                        mod_name = mod_name[:-5]
                    
                    self.file_to_mod[file_path] = mod_name
                    if mod_name not in self.dependencies:
                        self.dependencies[mod_name] = set()

                    with open(file_path, 'r', encoding='utf-8') as f:
                        content = f.read()
                        
                        # Match 'use crate::...'
                        uses = re.findall(r'use crate::([a-zA-Z0-9_:]+)', content)
                        for u in uses:
                            parts = u.split("::")
                            for i in range(len(parts), 0, -1):
                                candidate = "::".join(parts[:i])
                                self.dependencies[mod_name].add(candidate)
                        
                        # Match 'mod ...'
                        mods = re.findall(r'^(?:pub )?mod ([a-zA-Z0-9_]+);', content, re.MULTILINE)
                        for m in mods:
                            child_mod = f"{mod_name}::{m}"
                            self.dependencies[mod_name].add(child_mod)

    def gather_metrics(self):
        """Call hotspots.py in JSON mode."""
        try:
            res = subprocess.run(
                [".venv/Scripts/python.exe", "scripts/hotspots.py", "--mode", "json"],
                capture_output=True, text=True, check=True
            )
            data = json.loads(res.stdout)
            for item in data:
                file_path = os.path.normpath(item['name'])
                mod_name = None
                for path, m_name in self.file_to_mod.items():
                    if os.path.abspath(path) == os.path.abspath(file_path):
                        mod_name = m_name
                        break
                
                if not mod_name:
                    rel_path = os.path.relpath(file_path, "src")
                    mod_name = rel_path.replace(os.sep, "::").replace(".rs", "")
                    if mod_name.endswith("::mod"): mod_name = mod_name[:-5]
                    if mod_name == "lib": mod_name = "scratchpad"

                self.metrics[mod_name] = item
        except Exception as e:
            print(f"Warning: Could not gather complexity metrics: {e}")

    def gather_performance(self):
        """Call slowspots.py in JSON mode."""
        try:
            res = subprocess.run(
                [".venv/Scripts/python.exe", "scripts/slowspots.py", "--mode", "json"],
                capture_output=True, text=True, check=True
            )
            data = json.loads(res.stdout)
            
            for item in data:
                bench_name = item['name'].lower()
                mean_ms = item['mean_ns'] / 1_000_000.0
                perf_score = mean_ms * 10.0
                
                for mod in self.dependencies.keys():
                    short_mod = mod.split("::")[-1].lower()
                    if short_mod in bench_name:
                        if mod not in self.performance:
                            self.performance[mod] = {"score": 0.0, "items": []}
                        self.performance[mod]["score"] = max(self.performance[mod]["score"], perf_score)
                        self.performance[mod]["items"].append(item)
        except Exception as e:
            print(f"Warning: Could not gather performance metrics: {e}")

    def generate_mermaid(self) -> str:
        lines = ["graph TD"]
        valid_mods = set(self.dependencies.keys())
        for mod in sorted(valid_mods):
            comp_score = self.metrics.get(mod, {}).get('score', 0)
            perf_score = self.performance.get(mod, {}).get('score', 0)
            total_score = comp_score + perf_score
            sloc = self.metrics.get(mod, {}).get('sloc', 0)
            color = "#b5cea8" 
            if total_score > 300: color = "#d7ba7d"
            if total_score > 600: color = "#f44747"
            perf_warn = " ⚠️" if perf_score > 500 else ""
            label = f"{mod}<br/>(Score: {total_score:.0f}, SLOC: {sloc:.0f}){perf_warn}"
            node_id = mod.replace('::', '_')
            lines.append(f"    {node_id}[\"{label}\"]")
            lines.append(f"    style {node_id} fill:{color},stroke:#333,stroke-width:2px,color:#000")

        for source, targets in sorted(self.dependencies.items()):
            source_id = source.replace('::', '_')
            for t in sorted(targets):
                if t in valid_mods and t != source:
                    target_id = t.replace('::', '_')
                    lines.append(f"    {source_id} --> {target_id}")
        return "\n".join(lines)

    def generate_html(self) -> str:
        """Generate interactive Cytoscape.js visualization with split-circle scores."""
        nodes = []
        edges = []
        valid_mods = set(self.dependencies.keys())
        
        groups = set()
        for mod in valid_mods:
            parts = mod.split("::")
            for i in range(1, len(parts)):
                groups.add("::".join(parts[:i]))
        
        for g in sorted(groups):
            nodes.append({"data": {"id": g, "label": g, "is_group": True}})

        for mod in sorted(valid_mods):
            comp_score = float(self.metrics.get(mod, {}).get('score', 0))
            perf_data = self.performance.get(mod, {})
            perf_score = float(perf_data.get('score', 0))
            total_score = comp_score + perf_score
            sloc = int(self.metrics.get(mod, {}).get('sloc', 0))
            perf_items = perf_data.get('items', [])
            parent = "::".join(mod.split("::")[:-1]) if "::" in mod else None
            
            nodes.append({
                "data": {
                    "id": mod,
                    "label": mod.split("::")[-1],
                    "parent": parent,
                    "comp_score": comp_score,
                    "perf_score": perf_score,
                    "total_score": total_score,
                    "sloc": sloc,
                    "is_slow": perf_score > 500,
                    "perf_info": "<br/>".join([f"{p['name']}: {p['mean_ns']/1e6:.1f}ms" for p in perf_items]),
                    "signals": self.metrics.get(mod, {}).get('signals', 'stable')
                }
            })

        for source, targets in sorted(self.dependencies.items()):
            for t in sorted(targets):
                if t in valid_mods and t != source:
                    edges.append({"data": {"source": source, "target": t}})

        html_template = """
<!DOCTYPE html>
<html>
<head>
    <title>Scratchpad Architecture Map</title>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.26.0/cytoscape.min.js"></script>
    <script src="https://unpkg.com/dagre@0.8.5/dist/dagre.min.js"></script>
    <script src="https://unpkg.com/cytoscape-dagre@2.5.0/cytoscape-dagre.js"></script>
    <style>
        body { font-family: 'Segoe UI', sans-serif; background: #1e1e1e; color: #d4d4d4; margin: 0; overflow: hidden; }
        #cy { width: 100vw; height: 100vh; display: block; }
        .info-panel {
            position: absolute; top: 20px; right: 20px; width: 350px;
            background: rgba(37, 37, 38, 0.95); border: 1px solid #3e3e3e;
            padding: 15px; border-radius: 4px; box-shadow: 0 4px 10px rgba(0,0,0,0.5);
            pointer-events: auto; opacity: 0; transition: opacity 0.2s; z-index: 100;
            max-height: 90vh; overflow-y: auto;
        }
        .legend {
            position: absolute; bottom: 20px; left: 20px;
            background: rgba(37, 37, 38, 0.9); border: 1px solid #3e3e3e;
            padding: 10px; border-radius: 4px; font-size: 0.8em; z-index: 100;
        }
        .legend-item { display: flex; align-items: center; margin: 5px 0; }
        .color-box { width: 15px; height: 15px; margin-right: 10px; border-radius: 2px; }
        .split-box { width: 15px; height: 15px; margin-right: 10px; border-radius: 50%%; overflow: hidden; position: relative; border: 1px solid #555; }
        .top-half { position: absolute; top: 0; width: 100%%; height: 50%%; background: #f44747; }
        .bottom-half { position: absolute; bottom: 0; width: 100%%; height: 50%%; background: #b5cea8; }
        h1 { position: absolute; top: 10px; left: 20px; color: #569cd6; font-weight: 300; z-index: 100; margin: 0; }
        .dep-list { font-size: 0.85em; margin-left: 10px; line-height: 1.4; }
        .dep-item { cursor: pointer; color: #569cd6; text-decoration: underline; display: block; }
        .dep-item:hover { color: #9cdcfe; }
    </style>
</head>
<body>
    <h1>Scratchpad Map</h1>
    <div id="cy"></div>
    <div id="info" class="info-panel">
        <h3 id="info-title" style="margin-top:0; color: #ce9178;">Module Name</h3>
        <p><strong>Total Impact:</strong> <span id="info-total">0</span></p>
        <p><strong>Complexity Score (Top):</strong> <span id="info-comp">0</span></p>
        <p><strong>Performance Score (Bottom):</strong> <span id="info-perf-score">0</span></p>
        <p><strong>Lines of Code:</strong> <span id="info-sloc">0</span></p>
        <p><strong>Signals:</strong> <span id="info-signals">-</span></p>
        
        <div id="info-perf" style="display:none; color:#f44747; border-top: 1px solid #3e3e3e; padding-top:10px;">
            <strong>Slowspots:</strong><br/>
            <span id="info-perf-list" class="dep-list"></span>
        </div>

        <div id="info-deps" style="display:none; margin-top:10px; border-top: 1px solid #3e3e3e; padding-top:10px;">
            <strong style="color: #00ff00;">Depends On:</strong><br/>
            <div id="info-deps-list" class="dep-list"></div>
        </div>

        <div id="info-rev-deps" style="display:none; margin-top:10px; border-top: 1px solid #3e3e3e; padding-top:10px;">
            <strong style="color: #ff0000;">Depended By:</strong><br/>
            <div id="info-rev-deps-list" class="dep-list"></div>
        </div>
    </div>
    <div class="legend">
        <div class="legend-item"><div class="split-box"><div class="top-half"></div><div class="bottom-half"></div></div> Top: Complexity / Bottom: Performance</div>
        <div class="legend-item"><div class="color-box" style="background:#b5cea8"></div> Healthy / Low Risk</div>
        <div class="legend-item"><div class="color-box" style="background:#d7ba7d"></div> Moderate Risk</div>
        <div class="legend-item"><div class="color-box" style="background:#f44747"></div> Critical / High Risk</div>
        <div class="legend-item"><div class="color-box" style="background:#00ff00"></div> Click: Items Depends On (Dependency)</div>
        <div class="legend-item"><div class="color-box" style="background:#ff0000"></div> Click: Items Depended By (Dependent)</div>
        <div style="margin-top:10px; color:#888">Node Size = Total Impact (Comp + Perf)</div>
    </div>

    <script>
        const elements = %s;
        let selectedNodeId = null;
        
        function getComplexityColor(score) {
            if (score > 300) return '#f44747';
            if (score > 150) return '#d7ba7d';
            return '#b5cea8';
        }

        function getPerfColor(score) {
            if (score > 500) return '#f44747';
            if (score > 200) return '#d7ba7d';
            return '#b5cea8';
        }

        function updateInfoPanel(node) {
            document.getElementById('info-title').innerText = node.data('id');
            document.getElementById('info-total').innerText = (node.data('total_score') || 0).toFixed(1);
            document.getElementById('info-comp').innerText = (node.data('comp_score') || 0).toFixed(1);
            document.getElementById('info-perf-score').innerText = (node.data('perf_score') || 0).toFixed(1);
            document.getElementById('info-sloc').innerText = node.data('sloc') || 0;
            document.getElementById('info-signals').innerText = node.data('signals') || 'stable';
            
            const perfPanel = document.getElementById('info-perf');
            if (node.data('is_slow')) {
                perfPanel.style.display = 'block';
                document.getElementById('info-perf-list').innerHTML = node.data('perf_info');
            } else {
                perfPanel.style.display = 'none';
            }

            const depsPanel = document.getElementById('info-deps');
            const revDepsPanel = document.getElementById('info-rev-deps');
            
            if (selectedNodeId === node.id()) {
                const outgoers = node.outgoers().nodes().sort((a, b) => a.id().localeCompare(b.id()));
                const incomers = node.incomers().nodes().sort((a, b) => a.id().localeCompare(b.id()));
                
                if (outgoers.length > 0) {
                    depsPanel.style.display = 'block';
                    document.getElementById('info-deps-list').innerHTML = outgoers.map(n => 
                        `<span class="dep-item" onclick="selectNodeById('${n.id()}')">${n.id()}</span>`
                    ).join('');
                } else {
                    depsPanel.style.display = 'none';
                }

                if (incomers.length > 0) {
                    revDepsPanel.style.display = 'block';
                    document.getElementById('info-rev-deps-list').innerHTML = incomers.map(n => 
                        `<span class="dep-item" onclick="selectNodeById('${n.id()}')">${n.id()}</span>`
                    ).join('');
                } else {
                    revDepsPanel.style.display = 'none';
                }
            } else {
                depsPanel.style.display = 'none';
                revDepsPanel.style.display = 'none';
            }
        }

        function selectNodeById(id) {
            const node = cy.getElementById(id);
            if (node.length > 0) {
                cy.animate({ center: { eles: node }, zoom: 1.5 }, { duration: 500 });
                node.trigger('tap');
            }
        }

        const cy = cytoscape({
            container: document.getElementById('cy'),
            elements: elements,
            style: [
                {
                    selector: 'node',
                    style: {
                        'label': 'data(label)',
                        'text-valign': 'center',
                        'text-halign': 'center',
                        'color': '#000',
                        'font-size': '10px',
                        'background-color': '#333',
                        'width': (ele) => Math.max(40, Math.sqrt(ele.data('total_score') || 0) * 5),
                        'height': (ele) => Math.max(40, Math.sqrt(ele.data('total_score') || 0) * 5),
                    }
                },
                {
                    selector: 'node[is_group]',
                    style: {
                        'text-valign': 'top',
                        'text-halign': 'center',
                        'color': '#888',
                        'background-color': '#252526',
                        'border-width': 1,
                        'border-color': '#3e3e3e',
                        'shape': 'round-rectangle',
                        'padding': 15,
                        'width': 'auto',
                        'height': 'auto'
                    }
                },
                {
                    selector: 'node[!is_group]',
                    style: {
                        'shape': 'ellipse',
                        'pie-size': '100%%',
                        'pie-1-background-color': (ele) => getComplexityColor(ele.data('comp_score')),
                        'pie-1-background-size': 50,
                        'pie-2-background-color': (ele) => getPerfColor(ele.data('perf_score')),
                        'pie-2-background-size': 50,
                        'border-width': 1,
                        'border-color': '#333'
                    }
                },
                {
                    selector: 'edge',
                    style: {
                        'width': 2,
                        'line-color': '#3e3e3e',
                        'target-arrow-color': '#3e3e3e',
                        'target-arrow-shape': 'triangle',
                        'curve-style': 'bezier',
                        'opacity': 0.6
                    }
                },
                {
                    selector: '.dimmed',
                    style: {
                        'opacity': 0.1,
                        'text-opacity': 0.1
                    }
                },
                {
                    selector: '.depends-on-green',
                    style: {
                        'line-color': '#00ff00',
                        'target-arrow-color': '#00ff00',
                        'width': 4,
                        'opacity': 1,
                        'z-index': 10
                    }
                },
                {
                    selector: '.depended-by-red',
                    style: {
                        'line-color': '#ff0000',
                        'target-arrow-color': '#ff0000',
                        'width': 4,
                        'opacity': 1,
                        'z-index': 10
                    }
                },
                {
                    selector: '.highlight-green',
                    style: {
                        'border-width': 6,
                        'border-color': '#00ff00',
                        'opacity': 1,
                        'text-opacity': 1
                    }
                },
                {
                    selector: '.highlight-red',
                    style: {
                        'border-width': 6,
                        'border-color': '#ff0000',
                        'opacity': 1,
                        'text-opacity': 1
                    }
                },
                {
                    selector: '.highlight-center',
                    style: {
                        'border-width': 6,
                        'border-color': '#569cd6',
                        'opacity': 1,
                        'text-opacity': 1
                    }
                }
            ],
            layout: {
                name: 'dagre',
                padding: 50,
                spacingFactor: 1.2
            }
        });

        cy.on('mouseover', 'node[!is_group]', function(evt) {
            const node = evt.target;
            updateInfoPanel(node);
            document.getElementById('info').style.opacity = 1;
        });

        cy.on('mouseout', 'node', function() {
            if (!selectedNodeId) {
                document.getElementById('info').style.opacity = 0;
            } else {
                // If something is selected, show that info instead of nothing
                const selected = cy.getElementById(selectedNodeId);
                if (selected.length > 0) updateInfoPanel(selected);
            }
        });

        cy.on('tap', 'node[!is_group]', function(evt) {
            const node = evt.target;
            selectedNodeId = node.id();
            
            cy.elements().removeClass('dimmed highlight-green highlight-red highlight-center depends-on-green depended-by-red');
            cy.elements().addClass('dimmed');
            node.removeClass('dimmed').addClass('highlight-center');
            
            const outgoers = node.outgoers();
            outgoers.removeClass('dimmed');
            outgoers.edges().addClass('depends-on-green');
            outgoers.nodes().addClass('highlight-green');
            
            const incomers = node.incomers();
            incomers.removeClass('dimmed');
            incomers.edges().addClass('depended-by-red');
            incomers.nodes().addClass('highlight-red');

            updateInfoPanel(node);
            document.getElementById('info').style.opacity = 1;
        });

        cy.on('tap', function(evt) {
            if (evt.target === cy) {
                selectedNodeId = null;
                cy.elements().removeClass('dimmed highlight-green highlight-red highlight-center depends-on-green depended-by-red');
                document.getElementById('info').style.opacity = 0;
            }
        });
    </script>
</body>
</html>
"""
        elements_json = json.dumps(nodes + edges)
        return html_template % elements_json

def main():
    parser = argparse.ArgumentParser(description="Generate Integrated Architecture Map")
    parser.add_argument("--output", choices=["mermaid", "markdown", "display"], default="markdown")
    args = parser.parse_args()

    mapper = ArchitectureMapper()
    mapper.extract_dependencies("src")
    mapper.gather_metrics()
    mapper.gather_performance()

    if args.output == "display":
        html = mapper.generate_html()
        output_file = "architecture_map.html"
        with open(output_file, "w", encoding="utf-8") as f:
            f.write(html)
        print(f"Interactive map saved to: {os.path.abspath(output_file)}")
    elif args.output == "mermaid":
        print(mapper.generate_mermaid())
    else:
        print("# Integrated Architecture Map\n")
        print("This map integrates module dependencies, complexity (color), and performance (⚠️).\n")
        print("```mermaid")
        print(mapper.generate_mermaid())
        print("```")

if __name__ == "__main__":
    main()
