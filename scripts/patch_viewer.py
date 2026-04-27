import os
import sys

index_path = r"D:\Code\scratchpad\viewer\index.html"
js_path = r"D:\Code\scratchpad\viewer\data-viewer.js"

if not os.path.exists(index_path) or not os.path.exists(js_path):
    print("Viewer files not found.", file=sys.stderr)
    sys.exit(1)

with open(index_path, "r", encoding="utf-8") as f:
    index_html = f.read()

if 'id="overview-charts"' not in index_html:
    index_html = index_html.replace(
        '<div id="overview-summary" class="summary-grid"></div>',
        '<div id="overview-summary" class="summary-grid"></div>\n            <div id="overview-charts" class="chart-grid"></div>'
    )
    with open(index_path, "w", encoding="utf-8") as f:
        f.write(index_html)

with open(js_path, "r", encoding="utf-8") as f:
    js_code = f.read()

if 'renderOverviewCharts();' not in js_code:
    js_code = js_code.replace(
        'renderSummary("overview-summary", [',
        'renderOverviewCharts();\n        renderSummary("overview-summary", ['
    )
    
    chart_fn = """
    function renderOverviewCharts() {
        const container = byId("overview-charts");
        if (!container) return;

        // Correctness Chart (Pie or Horizontal Bar)
        const cPassed = state.correctness?.summary?.passed || 0;
        const cFailed = state.correctness?.summary?.failed || 0;
        const cUnknown = state.correctness?.summary?.unknown || 0;
        const cTotal = cPassed + cFailed + cUnknown || 1;

        // Quality Chart
        const qHotspots = state.hotspots?.length || 0;
        const qClones = state.clones?.length || 0;
        const qTotal = qHotspots + qClones || 1;

        // Map Risk
        let mGood = 0, mWarn = 0, mBad = 0;
        const modules = state.map?.modules || [];
        modules.forEach(m => {
            const r = m.metrics?.total_score || 0;
            if (r > 600) mBad++;
            else if (r > 300) mWarn++;
            else mGood++;
        });
        const mTotal = mGood + mWarn + mBad || 1;
        
        // Performance Chart
        const speedSummary = state.speedReport?.summary || {};
        const pSearch = speedSummary.search_scenarios || 0;
        const pEditor = speedSummary.editor_scenarios || 0;
        const pTabs = speedSummary.tabs_and_splits_scenarios || 0;
        const pTotal = pSearch + pEditor + pTabs || 1;

        container.innerHTML = `
            <div class="panel-card chart-panel">
                <div><h3>Quality Summary</h3><p class="chart-caption">Hotspots vs Clones.</p></div>
                <div style="height: 30px; display: flex; border-radius: 8px; overflow: hidden; margin-top: 10px;">
                    <div style="width: ${(qHotspots / qTotal) * 100}%; background: var(--warn);" title="Hotspots: ${qHotspots}"></div>
                    <div style="width: ${(qClones / qTotal) * 100}%; background: var(--purple);" title="Clones: ${qClones}"></div>
                </div>
                <div class="chart-legend" style="margin-top: 10px;">
                    <span class="chart-legend__item"><span style="color:var(--warn)">&#9632;</span> Hotspots (${qHotspots})</span>
                    <span class="chart-legend__item"><span style="color:var(--purple)">&#9632;</span> Clones (${qClones})</span>
                </div>
            </div>

            <div class="panel-card chart-panel">
                <div><h3>Performance Scenarios</h3><p class="chart-caption">Breakdown by benchmark domain.</p></div>
                <div style="height: 30px; display: flex; border-radius: 8px; overflow: hidden; margin-top: 10px;">
                    <div style="width: ${(pSearch / pTotal) * 100}%; background: var(--accent);" title="Search: ${pSearch}"></div>
                    <div style="width: ${(pEditor / pTotal) * 100}%; background: var(--good);" title="Editor: ${pEditor}"></div>
                    <div style="width: ${(pTabs / pTotal) * 100}%; background: var(--muted);" title="Tabs & Splits: ${pTabs}"></div>
                </div>
                <div class="chart-legend" style="margin-top: 10px;">
                    <span class="chart-legend__item"><span style="color:var(--accent)">&#9632;</span> Search (${pSearch})</span>
                    <span class="chart-legend__item"><span style="color:var(--good)">&#9632;</span> Editor (${pEditor})</span>
                    <span class="chart-legend__item"><span style="color:var(--muted)">&#9632;</span> Tabs & Splits (${pTabs})</span>
                </div>
            </div>

            <div class="panel-card chart-panel">
                <div><h3>Correctness Health</h3><p class="chart-caption">Test execution results.</p></div>
                <div style="height: 30px; display: flex; border-radius: 8px; overflow: hidden; margin-top: 10px;">
                    <div style="width: ${(cPassed / cTotal) * 100}%; background: var(--good);" title="Passed: ${cPassed}"></div>
                    <div style="width: ${(cFailed / cTotal) * 100}%; background: var(--bad);" title="Failed: ${cFailed}"></div>
                    <div style="width: ${(cUnknown / cTotal) * 100}%; background: var(--muted);" title="Unknown: ${cUnknown}"></div>
                </div>
                <div class="chart-legend" style="margin-top: 10px;">
                    <span class="chart-legend__item"><span style="color:var(--good)">&#9632;</span> Passed (${cPassed})</span>
                    <span class="chart-legend__item"><span style="color:var(--bad)">&#9632;</span> Failed (${cFailed})</span>
                    <span class="chart-legend__item"><span style="color:var(--muted)">&#9632;</span> Unknown (${cUnknown})</span>
                </div>
            </div>
            
            <div class="panel-card chart-panel">
                <div><h3>Module Architecture Risk</h3><p class="chart-caption">Modules grouped by total risk score.</p></div>
                <div style="height: 30px; display: flex; border-radius: 8px; overflow: hidden; margin-top: 10px;">
                    <div style="width: ${(mGood / mTotal) * 100}%; background: var(--good);" title="Good: ${mGood}"></div>
                    <div style="width: ${(mWarn / mTotal) * 100}%; background: var(--warn);" title="Warn: ${mWarn}"></div>
                    <div style="width: ${(mBad / mTotal) * 100}%; background: var(--bad);" title="Bad: ${mBad}"></div>
                </div>
                <div class="chart-legend" style="margin-top: 10px;">
                    <span class="chart-legend__item"><span style="color:var(--good)">&#9632;</span> Low Risk (${mGood})</span>
                    <span class="chart-legend__item"><span style="color:var(--warn)">&#9632;</span> Medium Risk (${mWarn})</span>
                    <span class="chart-legend__item"><span style="color:var(--bad)">&#9632;</span> High Risk (${mBad})</span>
                </div>
            </div>
        `;
    }
"""
    js_code += "\n" + chart_fn
    with open(js_path, "w", encoding="utf-8") as f:
        f.write(js_code)

print("Patch applied.")
