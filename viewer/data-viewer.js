(function () {
    const viewerVersion = window.SCRATCHPAD_VIEWER_VERSION || "dev";
    const sources = {
        catalog: `../target/analysis/measurement_catalog.json?v=${viewerVersion}`,
        runs: `../target/analysis/measurement_runs.json?v=${viewerVersion}`,
        hotspots: `../target/analysis/hotspots.json?v=${viewerVersion}`,
        slowspots: `../target/analysis/slowspots.json?v=${viewerVersion}`,
        searchSpeed: `../target/analysis/search_speed.json?v=${viewerVersion}`,
        capacityReport: `../target/analysis/capacity_report.json?v=${viewerVersion}`,
        resourceProfiles: `../target/analysis/resource_profiles.json?v=${viewerVersion}`,
        speedReport: `../target/analysis/speed_efficiency_report.json?v=${viewerVersion}`,
        clones: `../target/analysis/clones.json?v=${viewerVersion}`,
        map: `../target/analysis/map.json?v=${viewerVersion}`,
        flamegraphs: `../target/analysis/flamegraphs.json?v=${viewerVersion}`,
        correctness: `../target/analysis/correctness_review.json?v=${viewerVersion}`,
    };

    const state = {
        catalog: null,
        runs: [],
        hotspots: [],
        slowspots: [],
        searchSpeed: [],
        capacityReport: null,
        resourceProfiles: null,
        speedReport: null,
        clones: [],
        map: null,
        flamegraphs: [],
        correctness: null,
        selectedModule: null,
        selectedFlamegraph: null,
        selectedRun: null,
        selectedLayer: null,
        lastObservedFinishedRun: null,
        mapZoom: 0.65,
        mapLayout: 'folder',
        mapMetric: 'total_score',
        focusMode: false,
        overviewRiskMode: 'top',
        overviewRiskFilter: 'all',
        expandedQualityKey: null,
    };

    const formatNumber = new Intl.NumberFormat(undefined, {
        maximumFractionDigits: 2,
    });

    const searchModeColors = {
        active: "#6fd0ff",
        current: "#f3c969",
        all: "#c7a6ff",
    };

    const searchLatencyColors = {
        completion: "#6fd0ff",
        first_response: "#7ddc9b",
    };

    const searchLatencyDash = {
        completion: "",
        first_response: "8 6",
    };

    const riskMetricLabels = [
        ["maintainability_risk", "Maintainability"],
        ["change_risk", "Change"],
        ["performance_risk", "Performance"],
        ["correctness_risk", "Correctness"],
        ["architectural_risk", "Architecture"],
    ];

    function byId(id) {
        return document.getElementById(id);
    }

    function escapeHtml(value) {
        return String(value ?? "")
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#039;");
    }

    function riskClass(value, warn, bad) {
        if (value >= bad) {
            return "risk-bad";
        }
        if (value >= warn) {
            return "risk-warn";
        }
        return "risk-good";
    }

    function metricCard(label, value) {
        return `<div class="metric-card"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong></div>`;
    }

    function renderSummary(targetId, cards) {
        byId(targetId).innerHTML = cards.join("");
    }

    function renderTable(targetId, headers, rows) {
        const head = headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("");
        const body = rows.length
            ? rows.join("")
            : `<tr><td colspan="${headers.length}" class="muted">No data loaded.</td></tr>`;
        byId(targetId).innerHTML = `<table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table>`;
    }

    function matchesFilter(item, query) {
        if (!query) {
            return true;
        }
        return JSON.stringify(item).toLowerCase().includes(query.toLowerCase());
    }

    function renderHotspots() {
        const query = byId("hotspots-filter").value;
        const filtered = state.hotspots.filter((item) => matchesFilter(item, query));
        const worst = state.hotspots[0];
        const files = new Set(state.hotspots.filter((item) => item.kind === "unit").map((item) => item.name));
        const largeFiles = state.hotspots.filter((item) => Number(item.sloc || 0) >= 150).length;

        renderSummary("hotspots-summary", [
            metricCard("Records", state.hotspots.length),
            metricCard("Files", files.size),
            metricCard("Worst quality", worst ? formatNumber.format(qualityScore(worst)) : "-"),
            metricCard("Large items", largeFiles),
            metricCard("Worst item", worst ? worst.name.split(/[\\/]/).pop() : "-"),
        ]);

        renderTable(
            "hotspots-table",
            ["Rank", "Kind", "Name", "Quality", "Cog", "Cyc", "MI", "Halstead Effort", "SLOC", "Signals"],
            filtered.map((item, index) => {
                const score = qualityScore(item);
                const scoreClass = riskClass(score, 300, 600);
                return `<tr>
                    <td>${index + 1}</td>
                    <td><span class="pill">${escapeHtml(item.kind)}</span></td>
                    <td><code>${escapeHtml(item.name)}</code><div class="muted">line ${escapeHtml(item.start_line)}</div></td>
                    <td class="${scoreClass}">${formatNumber.format(score)}</td>
                    <td>${formatNumber.format(item.cognitive)}</td>
                    <td>${formatNumber.format(item.cyclomatic)}</td>
                    <td>${formatNumber.format(item.mi)}</td>
                    <td>${formatNumber.format(item.effort || 0)}</td>
                    <td>${formatNumber.format(item.sloc)}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
    }

    function qualityScore(item) {
        return Number(item.quality_score ?? item.score ?? 0);
    }

    function normalizePath(value) {
        return String(value || "").replaceAll("\\", "/").toLowerCase();
    }

    function renderClones() {
        const query = byId("clones-filter").value;
        const filtered = state.clones.filter((item) => matchesFilter(item, query));
        const totalInstances = state.clones.reduce((sum, item) => sum + item.instances.length, 0);
        const crossFileCount = state.clones.filter((item) => (item.file_count || 0) >= 2).length;
        const widest = state.clones.reduce((max, item) => Math.max(max, item.max_line_span || 0), 0);
        const astCount = state.clones.filter((item) => item.engine === "ast").length;

        renderSummary("clones-summary", [
            metricCard("Clone Groups", state.clones.length),
            metricCard("Total Instances", totalInstances),
            metricCard("Avg Instances", state.clones.length ? (totalInstances / state.clones.length).toFixed(1) : "-"),
            metricCard("Cross-file Groups", crossFileCount),
            metricCard("AST Groups", astCount),
            metricCard("Widest Span", widest ? `${widest} lines` : "-"),
        ]);

        renderTable(
            "clones-table",
            ["Engine", "Group Hash", "Instances", "Files", "Score", "Token Count", "Signals", "Locations"],
            filtered.map((item) => {
                const locations = item.instances.map((inst) =>
                    `<div><code>${escapeHtml(inst.file_path)}:${inst.start_line}-${inst.end_line}</code></div>`
                ).join("");
                const scoreClass = riskClass(item.score || 0, 20, 40);

                return `<tr>
                    <td><span class="pill">${escapeHtml(item.engine || "token")}</span></td>
                    <td><code>${escapeHtml(item.hash.substring(0, 8))}</code></td>
                    <td>${item.instances.length}</td>
                    <td>${item.file_count ?? "-"}</td>
                    <td class="${scoreClass}">${formatNumber.format(item.score)}</td>
                    <td>${item.token_count}</td>
                    <td>${renderPills(item.signals)}</td>
                    <td class="small-text">${locations}</td>
                </tr>`;
            })
        );
    }

    function renderSlowspots() {
        const query = byId("slowspots-filter").value;
        const filtered = state.slowspots.filter((item) => matchesFilter(item, query));
        const worst = state.slowspots[0];
        const slowCount = state.slowspots.filter((item) => item.mean_ns / 1_000_000 > item.threshold_ms).length;
        const mappedCount = state.slowspots.filter((item) => item.targets && item.targets.length).length;

        renderSummary("slowspots-summary", [
            metricCard("Benchmarks", state.slowspots.length),
            metricCard("Mapped", mappedCount),
            metricCard("Over threshold", slowCount),
            metricCard("Worst mean", worst ? `${formatNumber.format(worst.mean_ns / 1_000_000)} ms` : "-"),
        ]);

        renderTable(
            "slowspots-table",
            ["Benchmark", "Family", "Kind", "Mean", "Median", "Dispersion", "Threshold", "Profiles", "Targets", "Signals"],
            filtered.map((item) => {
                const meanMs = item.mean_ns / 1_000_000;
                const medianMs = item.median_ns / 1_000_000;
                const dispersionMs = item.dispersion_ns == null ? null : item.dispersion_ns / 1_000_000;
                const scoreClass = meanMs > item.threshold_ms ? "risk-bad" : "risk-good";
                return `<tr>
                    <td><code>${escapeHtml(item.name)}</code></td>
                    <td><span class="pill">${escapeHtml(item.workload_family || "unmapped")}</span></td>
                    <td><span class="pill">${escapeHtml(item.benchmark_kind)}</span></td>
                    <td class="${scoreClass}">${formatNumber.format(meanMs)} ms</td>
                    <td>${formatNumber.format(medianMs)} ms</td>
                    <td>${dispersionMs == null ? "-" : `${formatNumber.format(dispersionMs)} ms`}<div class="muted">${escapeHtml(item.dispersion_label || "median_abs_dev")}</div></td>
                    <td>${formatNumber.format(item.threshold_ms)} ms</td>
                    <td>${renderPills(item.matching_flamegraphs || [])}</td>
                    <td>${renderPills(item.targets || [])}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
    }

    function renderSearchSpeed() {
        const query = byId("search-speed-filter").value;
        const filtered = state.searchSpeed.filter((item) => matchesFilter(item, query));
        const benchmarkKeys = new Set(state.searchSpeed.map((item) => item.benchmark_key));
        const modes = new Set(state.searchSpeed.map((item) => item.mode));
        const firstResponseCount = state.searchSpeed.filter((item) => item.latency_kind === "first_response").length;
        const worst = state.searchSpeed[0];
        const overBudget = state.searchSpeed.filter((item) => item.mean_ns / 1_000_000 > item.threshold_ms).length;
        const bestThroughput = state.searchSpeed.reduce((max, item) => {
            return Math.max(max, item.throughput_mb_s || 0);
        }, 0);

        renderSummary("search-speed-summary", [
            metricCard("Records", state.searchSpeed.length),
            metricCard("Scenarios", benchmarkKeys.size),
            metricCard("Modes", modes.size),
            metricCard("First-response", firstResponseCount),
            metricCard("Over budget", overBudget),
            metricCard("Slowest mean", worst ? `${formatNumber.format(worst.mean_ns / 1_000_000)} ms` : "-"),
            metricCard("Best throughput", bestThroughput ? `${formatNumber.format(bestThroughput)} MB/s` : "-"),
        ]);

        renderSearchSpeedCharts(filtered);

        renderTable(
            "search-speed-table",
            ["Scenario", "Family", "Mode", "Latency", "Axis", "Param", "Corpus", "Mean", "Median", "Profiles", "Efficiency", "Targets", "Signals"],
            filtered.map((item) => {
                const meanMs = item.mean_ns / 1_000_000;
                const medianMs = item.median_ns / 1_000_000;
                const meanClass = meanMs > item.threshold_ms ? "risk-bad" : "risk-good";
                const nsPerKb = item.ns_per_kb == null ? "-" : `${formatNumber.format(item.ns_per_kb)} ns/KB`;
                const detailBits = [];
                if (item.description) detailBits.push(item.description);
                if (item.response_match_limit != null) detailBits.push(`preview limit ${item.response_match_limit} matches`);
                const detail = detailBits.length ? `<div class="muted">${escapeHtml(detailBits.join(" • "))}</div>` : "";
                const corpus = [];
                if (item.item_count != null) corpus.push(`${item.item_count} items`);
                if (item.bytes_per_item != null) corpus.push(`${formatNumber.format(item.bytes_per_item / 1024)} KB/item`);
                if (item.total_bytes != null) corpus.push(`${formatNumber.format(item.total_bytes / (1024 * 1024))} MB total`);

                return `<tr>
                    <td><code>${escapeHtml(item.scenario_label || item.name)}</code>${detail}</td>
                    <td><span class="pill">${escapeHtml(item.workload_family || "search")}</span></td>
                    <td><span class="pill">${escapeHtml(item.mode || "unknown")}</span></td>
                    <td><span class="pill">${escapeHtml(item.latency_kind || "completion")}</span></td>
                    <td><span class="pill">${escapeHtml(item.scaling_axis || "aggregate_size")}</span></td>
                    <td>${escapeHtml(item.parameter_label || "-")}</td>
                    <td>${escapeHtml(corpus.join(" • ") || "-")}</td>
                    <td class="${meanClass}">${formatNumber.format(meanMs)} ms<div class="muted">budget ${formatNumber.format(item.threshold_ms)} ms</div></td>
                    <td>${formatNumber.format(medianMs)} ms</td>
                    <td>${renderPills(item.matching_flamegraphs || [])}</td>
                    <td>${escapeHtml(nsPerKb)}</td>
                    <td>${renderPills(item.targets || [])}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
    }

    function renderSearchSpeedCharts(items) {
        const container = byId("search-speed-charts");
        if (!container) {
            return;
        }
        if (!items.length) {
            container.innerHTML = `<section class="panel-card chart-panel"><div class="chart-empty">No search speed data matches the current filter.</div></section>`;
            return;
        }

        const tabsSeries = buildSearchSpeedSeries(
            items,
            (item) => item.mode === "all" && item.scaling_axis === "aggregate_size",
            (item) => item.latency_kind,
            (key) => ({
                label: latencyLabel(key),
                shortLabel: latencyLabel(key),
                latencyKind: key,
                color: searchLatencyColors[key] || "#6fd0ff",
                dasharray: searchLatencyDash[key] || "",
                order: key === "completion" ? 0 : 1,
            })
        );

        const filesSeries = buildSearchSpeedSeries(
            items,
            (item) => item.mode === "current" && item.scaling_axis === "aggregate_size",
            (item) => item.latency_kind,
            (key) => ({
                label: latencyLabel(key),
                shortLabel: latencyLabel(key),
                latencyKind: key,
                color: searchLatencyColors[key] || "#6fd0ff",
                dasharray: searchLatencyDash[key] || "",
                order: key === "completion" ? 0 : 1,
            })
        );

        const fileSizeSeries = buildSearchSpeedSeries(
            items,
            (item) => item.scaling_axis === "file_size",
            (item) => `${item.mode}:${item.latency_kind}`,
            (key) => {
                const [mode, latencyKind] = key.split(":");
                const modeLabel = titleCase(mode);
                const latencyText = latencyKind === "first_response" ? "First response" : "Completion";
                const latencyOrder = latencyKind === "completion" ? 0 : 1;
                const modeOrder = { active: 0, current: 1, all: 2 }[mode] ?? 9;
                return {
                    label: `${modeLabel} ${latencyText}`,
                    shortLabel: modeLabel,
                    mode,
                    latencyKind,
                    color: searchModeColors[mode] || "#6fd0ff",
                    dasharray: searchLatencyDash[latencyKind] || "",
                    order: modeOrder * 2 + latencyOrder,
                };
            }
        );

        const dependencyMetrics = buildSearchDependencyMetrics(items);

        container.innerHTML = [
            buildSearchSpeedLineCard({
                title: "Tabs Against Time",
                subtitle: "All-open-tabs aggregate-size scenarios. Solid = completion, dashed = first response.",
                series: tabsSeries,
                insights: buildAggregateScopeInsights(tabsSeries),
                hardLimitText: "No hard limit observed in the measured tab range.",
            }),
            buildSearchSpeedLineCard({
                title: "Files Against Time",
                subtitle: "Current-workspace aggregate-size scenarios. Solid = completion, dashed = first response.",
                series: filesSeries,
                insights: buildAggregateScopeInsights(filesSeries),
                hardLimitText: "No hard limit observed in the measured file-count range.",
            }),
            buildSearchSpeedLineCard({
                title: "File Size Against Time",
                subtitle: "Active, Current, and All file-size scenarios. Color = mode, dashed = first response.",
                series: fileSizeSeries,
                insights: buildFileSizeInsights(fileSizeSeries),
                hardLimitText: "No hard limit observed; every file-size series completed its largest sampled input.",
            }),
            buildSearchDependencyCard(dependencyMetrics),
        ].join("");
    }

    function buildSearchSpeedSeries(items, predicate, keyFn, describeFn) {
        const groups = new Map();

        items.filter(predicate).forEach((item) => {
            const key = keyFn(item);
            if (!groups.has(key)) {
                groups.set(key, []);
            }
            groups.get(key).push(item);
        });

        return Array.from(groups.entries())
            .map(([key, group]) => {
                const ordered = [...group].sort((left, right) => (left.parameter_value ?? 0) - (right.parameter_value ?? 0));
                return {
                    key,
                    ...describeFn(key, ordered[0]),
                    points: ordered.map((item) => ({
                        xValue: item.parameter_value ?? 0,
                        xLabel: item.parameter_label || String(item.parameter_value ?? "-"),
                        meanMs: item.mean_ns / 1_000_000,
                        thresholdMs: item.threshold_ms,
                        throughput: item.throughput_mb_s || 0,
                    })),
                };
            })
            .sort((left, right) => (left.order ?? 0) - (right.order ?? 0));
    }

    function buildSearchSpeedLineCard({ title, subtitle, series, insights, hardLimitText }) {
        if (!series.length) {
            return `<section class="panel-card chart-panel"><div><h3>${escapeHtml(title)}</h3><p class="chart-caption">${escapeHtml(subtitle)}</p></div><div class="chart-empty">No matching records for this chart.</div></section>`;
        }

        const orderedX = Array.from(
            new Map(
                series
                    .flatMap((entry) => entry.points)
                    .sort((left, right) => left.xValue - right.xValue)
                    .map((point) => [point.xValue, point.xLabel])
            ).entries()
        );

        const allValues = series.flatMap((entry) => entry.points.map((point) => Math.max(point.meanMs, 0.001)));
        let minValue = Math.min(...allValues);
        let maxValue = Math.max(...allValues);
        if (minValue === maxValue) {
            minValue *= 0.5;
            maxValue *= 1.5;
        }
        const yTicks = buildLogTicks(minValue, maxValue);
        const yMin = yTicks[0];
        const yMax = yTicks[yTicks.length - 1];

        const width = 760;
        const height = 320;
        const left = 64;
        const right = 24;
        const top = 24;
        const bottom = 52;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const xStep = orderedX.length > 1 ? plotWidth / (orderedX.length - 1) : 0;
        const xLookup = new Map(
            orderedX.map(([value], index) => [value, orderedX.length === 1 ? left + plotWidth / 2 : left + index * xStep])
        );
        const logMin = Math.log10(yMin);
        const logMax = Math.log10(yMax);
        const yPosition = (value) => {
            const safeValue = Math.max(value, yMin);
            const ratio = (Math.log10(safeValue) - logMin) / Math.max(logMax - logMin, 0.0001);
            return top + plotHeight - ratio * plotHeight;
        };

        const gridLines = yTicks.map((tick) => {
            const y = yPosition(tick);
            return `<g>
                <line class="chart-grid-line" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label" x="${left - 10}" y="${y + 4}" text-anchor="end">${escapeHtml(formatAxisMs(tick))}</text>
            </g>`;
        }).join("");

        const xTicks = orderedX.map(([value, label]) => {
            const x = xLookup.get(value);
            return `<g>
                <line class="chart-axis-line" x1="${x}" y1="${height - bottom}" x2="${x}" y2="${height - bottom + 6}"></line>
                <text class="chart-tick-label" x="${x}" y="${height - bottom + 22}" text-anchor="middle">${escapeHtml(label)}</text>
            </g>`;
        }).join("");

        const seriesMarkup = series.map((entry) => {
            const path = entry.points
                .map((point, index) => `${index === 0 ? "M" : "L"} ${xLookup.get(point.xValue)} ${yPosition(point.meanMs)}`)
                .join(" ");
            const markers = entry.points.map((point) => {
                const x = xLookup.get(point.xValue);
                const y = yPosition(point.meanMs);
                const overBudget = point.meanMs > point.thresholdMs;
                return `<g>
                    <circle class="chart-point" cx="${x}" cy="${y}" r="5" stroke="${overBudget ? "#ff7474" : entry.color}" fill="#10151c"></circle>
                    ${overBudget ? `<circle class="chart-point--over" cx="${x}" cy="${y}" r="9"></circle>` : ""}
                </g>`;
            }).join("");
            return `<g>
                <path class="chart-series-line" d="${path}" stroke="${entry.color}"${entry.dasharray ? ` stroke-dasharray="${entry.dasharray}"` : ""}></path>
                ${markers}
            </g>`;
        }).join("");

        const legend = renderChartLegend(series);
        const insightMarkup = [...insights, hardLimitText]
            .filter(Boolean)
            .map((item) => `<li>${escapeHtml(item)}</li>`)
            .join("");

        return `<section class="panel-card chart-panel">
            <div>
                <h3>${escapeHtml(title)}</h3>
                <p class="chart-caption">${escapeHtml(subtitle)}</p>
            </div>
            <div class="chart-frame">
                <svg class="chart-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(title)}">
                    ${gridLines}
                    <line class="chart-axis-line" x1="${left}" y1="${top}" x2="${left}" y2="${height - bottom}"></line>
                    <line class="chart-axis-line" x1="${left}" y1="${height - bottom}" x2="${width - right}" y2="${height - bottom}"></line>
                    ${xTicks}
                    ${seriesMarkup}
                    <text class="chart-axis-label" x="${width / 2}" y="${height - 12}" text-anchor="middle">Scale</text>
                    <text class="chart-axis-label" x="18" y="${top + plotHeight / 2}" text-anchor="middle" transform="rotate(-90 18 ${top + plotHeight / 2})">Time (ms, log scale)</text>
                </svg>
            </div>
            <div class="chart-legend">${legend}</div>
            <ul class="chart-insights">${insightMarkup}</ul>
        </section>`;
    }

    function buildSearchDependencyCard(metrics) {
        if (!metrics.length) {
            return `<section class="panel-card chart-panel"><div><h3>Relative Dependency</h3><p class="chart-caption">Time multiplier when each growth axis doubles.</p></div><div class="chart-empty">No matching records for dependency analysis.</div></section>`;
        }

        const width = 760;
        const height = 320;
        const left = 60;
        const right = 24;
        const top = 24;
        const bottom = 52;
        const plotWidth = width - left - right;
        const plotHeight = height - top - bottom;
        const barValues = metrics.flatMap((item) => [item.completionMultiplier, item.firstResponseMultiplier].filter((value) => Number.isFinite(value)));
        const maxValue = Math.max(...barValues, 1);
        const yMax = Math.max(1.2, Math.ceil(maxValue * 1.2 * 10) / 10);
        const yTicks = buildLinearTicks(yMax);
        const groupWidth = plotWidth / metrics.length;
        const barWidth = Math.min(46, groupWidth * 0.28);
        const gap = Math.min(18, groupWidth * 0.08);
        const yPosition = (value) => top + plotHeight - (value / yMax) * plotHeight;

        const gridLines = yTicks.map((tick) => {
            const y = yPosition(tick);
            return `<g>
                <line class="chart-grid-line" x1="${left}" y1="${y}" x2="${width - right}" y2="${y}"></line>
                <text class="chart-tick-label" x="${left - 10}" y="${y + 4}" text-anchor="end">${escapeHtml(formatNumber.format(tick))}x</text>
            </g>`;
        }).join("");

        const bars = metrics.map((metric, index) => {
            const groupCenter = left + groupWidth * index + groupWidth / 2;
            const completionX = groupCenter - barWidth - gap / 2;
            const responseX = groupCenter + gap / 2;
            const completionHeight = (metric.completionMultiplier / yMax) * plotHeight;
            const responseHeight = (metric.firstResponseMultiplier / yMax) * plotHeight;
            const completionY = yPosition(metric.completionMultiplier);
            const responseY = yPosition(metric.firstResponseMultiplier);
            return `<g>
                <rect class="chart-bar--completion" x="${completionX}" y="${completionY}" width="${barWidth}" height="${completionHeight}" rx="8"></rect>
                <rect class="chart-bar--first-response" x="${responseX}" y="${responseY}" width="${barWidth}" height="${responseHeight}" rx="8"></rect>
                <text class="chart-value-label" x="${completionX + barWidth / 2}" y="${completionY - 8}" text-anchor="middle">${escapeHtml(formatNumber.format(metric.completionMultiplier))}x</text>
                <text class="chart-value-label" x="${responseX + barWidth / 2}" y="${responseY - 8}" text-anchor="middle">${escapeHtml(formatNumber.format(metric.firstResponseMultiplier))}x</text>
                <text class="chart-tick-label" x="${groupCenter}" y="${height - bottom + 22}" text-anchor="middle">${escapeHtml(metric.label)}</text>
            </g>`;
        }).join("");

        const legend = [
            { label: "Completion", color: searchLatencyColors.completion },
            { label: "First response", color: searchLatencyColors.first_response },
        ].map((item) => `<span class="chart-legend__item">
                <svg class="chart-legend__swatch" viewBox="0 0 28 10" aria-hidden="true"><line x1="1" y1="5" x2="27" y2="5" stroke="${item.color}" stroke-width="5"></line></svg>
                <span>${escapeHtml(item.label)}</span>
            </span>`).join("");

        const insights = buildDependencyInsights(metrics)
            .map((item) => `<li>${escapeHtml(item)}</li>`)
            .join("");

        return `<section class="panel-card chart-panel">
            <div>
                <h3>Relative Dependency</h3>
                <p class="chart-caption">Time multiplier when each growth axis doubles. 1.0x is flat, 2.0x is linear, above 2.0x means time degrades faster than the input grows.</p>
            </div>
            <div class="chart-frame">
                <svg class="chart-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="Relative dependency of search speed on tabs, files, and file size">
                    ${gridLines}
                    <line class="chart-axis-line" x1="${left}" y1="${top}" x2="${left}" y2="${height - bottom}"></line>
                    <line class="chart-axis-line" x1="${left}" y1="${height - bottom}" x2="${width - right}" y2="${height - bottom}"></line>
                    ${bars}
                    <text class="chart-axis-label" x="${width / 2}" y="${height - 12}" text-anchor="middle">Growth axis</text>
                    <text class="chart-axis-label" x="18" y="${top + plotHeight / 2}" text-anchor="middle" transform="rotate(-90 18 ${top + plotHeight / 2})">Time multiplier per 2x growth</text>
                </svg>
            </div>
            <div class="chart-legend">${legend}</div>
            <ul class="chart-insights">${insights}</ul>
        </section>`;
    }

    function buildAggregateScopeInsights(series) {
        const completion = series.find((item) => item.latencyKind === "completion");
        const firstResponse = series.find((item) => item.latencyKind === "first_response");
        const insights = [];

        if (completion) {
            const overBudget = completion.points.find((point) => point.meanMs > point.thresholdMs);
            insights.push(
                overBudget
                    ? `Completion crosses its budget at ${overBudget.xLabel}.`
                    : `Completion stays within budget through ${completion.points[completion.points.length - 1].xLabel}.`
            );
        }

        if (firstResponse) {
            const overBudget = firstResponse.points.find((point) => point.meanMs > point.thresholdMs);
            insights.push(
                overBudget
                    ? `First response crosses its budget at ${overBudget.xLabel}.`
                    : `First response stays within budget through ${firstResponse.points[firstResponse.points.length - 1].xLabel}.`
            );
        }

        const completionMultiplier = completion ? calculateDoublingMultiplier(completion.points) : null;
        const responseMultiplier = firstResponse ? calculateDoublingMultiplier(firstResponse.points) : null;
        if (Number.isFinite(completionMultiplier) && Number.isFinite(responseMultiplier)) {
            insights.push(
                `Completion is ${describeGrowth(completionMultiplier)} while first response is ${describeGrowth(responseMultiplier)}.`
            );
        }

        return insights;
    }

    function buildFileSizeInsights(series) {
        const completionSeries = series.filter((item) => item.latencyKind === "completion");
        const firstResponseSeries = series.filter((item) => item.latencyKind === "first_response");
        const insights = [];

        const completionBreaks = completionSeries
            .map((item) => {
                const overBudget = item.points.find((point) => point.meanMs > point.thresholdMs);
                return overBudget ? `${item.shortLabel} at ${overBudget.xLabel}` : null;
            })
            .filter(Boolean);
        insights.push(
            completionBreaks.length
                ? `Completion budget breaks start at ${completionBreaks.join("; ")}.`
                : "Completion stays within budget across all measured file-size series."
        );

        const firstResponseBreaks = firstResponseSeries
            .map((item) => {
                const overBudget = item.points.find((point) => point.meanMs > point.thresholdMs);
                return overBudget ? `${item.shortLabel} at ${overBudget.xLabel}` : null;
            })
            .filter(Boolean);
        insights.push(
            firstResponseBreaks.length
                ? `First response budget breaks start at ${firstResponseBreaks.join("; ")}.`
                : "First response stays within budget across Active, Current, and All."
        );

        const completionMultiplier = mean(
            completionSeries
                .map((item) => calculateDoublingMultiplier(item.points))
                .filter((value) => Number.isFinite(value))
        );
        const responseMultiplier = mean(
            firstResponseSeries
                .map((item) => calculateDoublingMultiplier(item.points))
                .filter((value) => Number.isFinite(value))
        );
        if (Number.isFinite(completionMultiplier) && Number.isFinite(responseMultiplier)) {
            insights.push(
                `Across modes, completion is ${describeGrowth(completionMultiplier)} while first response is ${describeGrowth(responseMultiplier)}.`
            );
        }

        return insights;
    }

    function buildSearchDependencyMetrics(items) {
        const dimensions = [
            {
                label: "Tabs",
                completionSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "all" && item.scaling_axis === "aggregate_size" && item.latency_kind === "completion",
                    () => "completion",
                    () => ({ order: 0 })
                ),
                firstResponseSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "all" && item.scaling_axis === "aggregate_size" && item.latency_kind === "first_response",
                    () => "first_response",
                    () => ({ order: 0 })
                ),
            },
            {
                label: "Files",
                completionSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "current" && item.scaling_axis === "aggregate_size" && item.latency_kind === "completion",
                    () => "completion",
                    () => ({ order: 0 })
                ),
                firstResponseSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.mode === "current" && item.scaling_axis === "aggregate_size" && item.latency_kind === "first_response",
                    () => "first_response",
                    () => ({ order: 0 })
                ),
            },
            {
                label: "File size",
                completionSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.scaling_axis === "file_size" && item.latency_kind === "completion",
                    (item) => item.mode,
                    (mode) => ({ order: { active: 0, current: 1, all: 2 }[mode] ?? 9 })
                ),
                firstResponseSeries: buildSearchSpeedSeries(
                    items,
                    (item) => item.scaling_axis === "file_size" && item.latency_kind === "first_response",
                    (item) => item.mode,
                    (mode) => ({ order: { active: 0, current: 1, all: 2 }[mode] ?? 9 })
                ),
            },
        ];

        return dimensions.map((dimension) => ({
            label: dimension.label,
            completionMultiplier: mean(
                dimension.completionSeries
                    .map((entry) => calculateDoublingMultiplier(entry.points))
                    .filter((value) => Number.isFinite(value))
            ),
            firstResponseMultiplier: mean(
                dimension.firstResponseSeries
                    .map((entry) => calculateDoublingMultiplier(entry.points))
                    .filter((value) => Number.isFinite(value))
            ),
        })).filter((item) => Number.isFinite(item.completionMultiplier) && Number.isFinite(item.firstResponseMultiplier));
    }

    function buildDependencyInsights(metrics) {
        const completionSorted = [...metrics].sort((left, right) => right.completionMultiplier - left.completionMultiplier);
        const responseSorted = [...metrics].sort((left, right) => right.firstResponseMultiplier - left.firstResponseMultiplier);
        const flattestResponse = [...metrics].sort((left, right) => left.firstResponseMultiplier - right.firstResponseMultiplier)[0];
        const insights = [];

        if (completionSorted[0]) {
            insights.push(
                `Completion depends most on ${completionSorted[0].label.toLowerCase()} growth at ${formatNumber.format(completionSorted[0].completionMultiplier)}x time per 2x scale.`
            );
        }
        if (responseSorted[0]) {
            insights.push(
                `First response depends most on ${responseSorted[0].label.toLowerCase()} growth at ${formatNumber.format(responseSorted[0].firstResponseMultiplier)}x time per 2x scale.`
            );
        }
        if (flattestResponse) {
            insights.push(
                `First response is flattest against ${flattestResponse.label.toLowerCase()}, which is consistent with the capped-result benchmark path.`
            );
        }

        return insights;
    }

    function renderSpeedReport() {
        const payload = state.speedReport || {};
        const summary = payload.summary || {};
        const triage = payload.triage || [];
        const sections = payload.sections || {};

        renderSummary("speed-report-summary", [
            metricCard("Search rows", summary.search_scenarios ?? "-"),
            metricCard("Editor rows", summary.editor_scenarios ?? "-"),
            metricCard("Tabs / splits", summary.tabs_and_splits_scenarios ?? "-"),
            metricCard("Capacity scenarios", summary.capacity_scenarios ?? "-"),
            metricCard("Over budget", summary.over_budget_latency ?? "-"),
            metricCard("Coverage gaps", summary.coverage_gaps ?? "-"),
            metricCard("Near ceilings", summary.near_failure_ceilings ?? "-"),
        ]);

        byId("speed-report-triage").innerHTML = renderSpeedTriageVisual(triage, payload.triage_summary || {});

        renderTable(
            "speed-report-search",
            ["Scenario", "Family", "Mean", "Budget", "Profiles", "Stability", "Ceiling", "Resource"],
            (sections.search || []).map((item) => `<tr>
                <td><code>${escapeHtml(item.scenario_label || item.scenario_id)}</code></td>
                <td><span class="pill">${escapeHtml(item.family || "search")}</span></td>
                <td>${formatNumber.format(item.mean_ms || 0)} ms</td>
                <td>${formatNumber.format(item.budget_ms || 0)} ms</td>
                <td>${renderPills(item.matching_flamegraphs || [])}</td>
                <td><span class="pill">${escapeHtml(item.stability || "stable")}</span></td>
                <td>${escapeHtml(item.last_known_failure_ceiling || "-")}</td>
                <td><span class="pill">${escapeHtml(item.suspected_limiting_resource || "cpu")}</span></td>
            </tr>`)
        );

        renderTable(
            "speed-report-editor",
            ["Scenario", "Family", "Mean", "Budget", "Profiles", "Signals", "Ceiling", "Resource"],
            [...(sections.editor_file_size || []), ...(sections.tabs_and_splits || [])].map((item) => `<tr>
                <td><code>${escapeHtml(item.scenario_label || item.scenario_id)}</code></td>
                <td><span class="pill">${escapeHtml(item.family || "unmapped")}</span></td>
                <td>${formatNumber.format(item.mean_ms || 0)} ms</td>
                <td>${formatNumber.format(item.budget_ms || 0)} ms</td>
                <td>${renderPills(item.matching_flamegraphs || [])}</td>
                <td>${renderPills(item.signals || [])}</td>
                <td>${escapeHtml(item.last_known_failure_ceiling || "-")}</td>
                <td><span class="pill">${escapeHtml(item.suspected_limiting_resource || "cpu")}</span></td>
            </tr>`)
        );

        renderTable(
            "speed-report-flamegraphs",
            ["Profile", "Role", "Available", "Families", "Benchmarks", "Covered scenarios", "Issue"],
            (sections.flamegraph_coverage || []).map((item) => `<tr>
                <td><code>${escapeHtml(item.name || item.id)}</code></td>
                <td><span class="pill">${escapeHtml(item.coverage_role || "report-driven")}</span></td>
                <td>${item.available ? "yes" : "no"}</td>
                <td>${renderPills(item.workload_families || [])}</td>
                <td>${renderPills(item.benchmark_keys || [])}</td>
                <td>${renderPills(item.covered_scenarios || [])}</td>
                <td>${escapeHtml(item.issue || "-")}</td>
            </tr>`)
        );

        const methodology = sections.methodology || [];
        byId("speed-report-methodology").innerHTML = methodology.length
            ? `<ul class="chart-insights">${methodology.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul>`
            : '<p class="muted">No methodology notes loaded.</p>';
    }

    function renderSpeedTriageVisual(triage, triageSummary = {}) {
        if (!triage.length) {
            return '<p class="muted">No coordinated triage data loaded.</p>';
        }
        const groups = new Map();
        triage.forEach((item) => {
            const key = [
                item.scenario_label || item.scenario_id || "Unknown scenario",
                item.family || "unmapped",
                item.suspected_limiting_resource || "resource",
                item.recommended_action || "",
            ].join("|");
            const group = groups.get(key) || {
                label: item.scenario_label || item.scenario_id || "Unknown scenario",
                family: item.family || "unmapped",
                resource: item.suspected_limiting_resource || "resource",
                action: item.recommended_action || "",
                count: 0,
                rankScore: 0,
                reasons: new Set(),
            };
            group.count += 1;
            group.rankScore = Math.max(group.rankScore, Number(item.rank_score || 0));
            pillValues(item.reason).forEach((reason) => group.reasons.add(reason));
            groups.set(key, group);
        });
        const cards = [...groups.values()]
            .sort((a, b) => b.rankScore - a.rankScore)
            .slice(0, 5);
        const maxScore = cards[0]?.rankScore || 1;
        const total = (triageSummary.critical || 0) + (triageSummary.watch || 0) + (triageSummary.ok || 0);
        const criticalPct = total ? ((triageSummary.critical || 0) / total) * 100 : 0;
        const watchPct = total ? ((triageSummary.watch || 0) / total) * 100 : 0;
        const okPct = Math.max(0, 100 - criticalPct - watchPct);
        return `<div class="triage-snapshot">
            <div class="triage-severity" aria-label="Triage severity distribution">
                <div class="triage-severity__track">
                    <span class="triage-severity__critical" style="width:${criticalPct}%"></span>
                    <span class="triage-severity__watch" style="width:${watchPct}%"></span>
                    <span class="triage-severity__ok" style="width:${okPct}%"></span>
                </div>
                <div class="triage-severity__legend">
                    <span><strong>${formatNumber.format(triageSummary.critical || 0)}</strong> critical</span>
                    <span><strong>${formatNumber.format(triageSummary.watch || 0)}</strong> watch</span>
                    <span><strong>${formatNumber.format(triageSummary.ok || 0)}</strong> ok</span>
                </div>
            </div>
            <div class="triage-visual">
                ${cards.map((item, index) => renderSpeedTriageCard(item, index, maxScore)).join("")}
            </div>
        </div>`;
    }

    function renderSpeedTriageCard(item, index, maxScore) {
        const scorePct = Math.max(6, Math.min(100, (item.rankScore / maxScore) * 100));
        const reasons = [...item.reasons].slice(0, 4);
        const profile = (item.action.match(/Inspect\s+([^\s]+)_profile/i) || [])[1];
        return `<article class="triage-card">
            <div class="triage-card__rank">${index + 1}</div>
            <div class="triage-card__body">
                <div class="triage-card__header">
                    <h3>${escapeHtml(item.label)}</h3>
                    <strong>${formatNumber.format(item.rankScore)}</strong>
                </div>
                <div class="triage-card__bar"><span style="width:${scorePct}%"></span></div>
                <div class="triage-card__chips">
                    <span class="pill">${escapeHtml(item.family)}</span>
                    <span class="pill">${escapeHtml(item.resource)}</span>
                    ${item.count > 1 ? `<span class="pill">${item.count} scale points</span>` : ""}
                    ${profile ? `<span class="pill">${escapeHtml(profile)}</span>` : ""}
                </div>
                <div class="triage-card__signals">
                    ${reasons.map((reason) => `<span>${escapeHtml(reason)}</span>`).join("")}
                </div>
            </div>
        </article>`;
    }

    function renderCapacityReport() {
        const payload = state.capacityReport || {};
        const summary = payload.summary || {};
        const scenarios = payload.scenarios || [];

        renderSummary("capacity-report-summary", [
            metricCard("Scenarios", summary.scenario_count ?? "-"),
            metricCard("Ceilings reached", summary.ceilings_reached ?? "-"),
            metricCard("Memory bound", summary.memory_bound_scenarios ?? "-"),
            metricCard("CPU bound", summary.cpu_bound_scenarios ?? "-"),
        ]);

        renderTable(
            "capacity-report-table",
            ["Scenario", "Failure mode", "Last OK", "First failure", "Resource", "Peak working set", "Growth", "Profiles", "Guidance"],
            scenarios.map((item) => `<tr>
                <td><code>${escapeHtml(item.scenario_label || item.scenario)}</code></td>
                <td><span class="pill">${escapeHtml(item.failure_mode || "not_reached")}</span></td>
                <td>${escapeHtml(item.last_successful_label || "-")}</td>
                <td>${escapeHtml(item.first_failure_label || "-")}</td>
                <td><span class="pill">${escapeHtml(item.suspected_limiting_resource || "cpu")}</span></td>
                <td>${escapeHtml(formatBytes(item.peak_working_set_bytes))}</td>
                <td>${escapeHtml(formatBytes(item.working_set_growth_bytes))}</td>
                <td>${renderPills(item.matching_flamegraphs || [])}</td>
                <td>${escapeHtml((item.diagnosis_guidance || []).join(" • ") || "-")}</td>
            </tr>`)
        );
    }

    function renderResourceProfiles() {
        const payload = state.resourceProfiles || {};
        const summary = payload.summary || {};
        const scenarios = payload.scenarios || [];
        const query = byId("resource-profiles-filter")?.value || "";
        const filteredScenarios = scenarios.filter((item) => matchesFilter(item, query));
        const sampleRows = filteredScenarios.flatMap((scenario) =>
            (scenario.samples || []).map((sample) => ({
                scenarioLabel: scenario.scenario_label || scenario.scenario,
                workloadFamily: scenario.workload_family || "unmapped",
                focus: scenario.focus || "resource",
                ...sample,
            }))
        );
        const worstElapsed = scenarios.reduce((max, item) => Math.max(max, item.max_elapsed_ms || 0), 0);
        const maxAllocated = scenarios.reduce((max, item) => Math.max(max, item.max_allocated_bytes || 0), 0);
        const maxWorkingSet = scenarios.reduce((max, item) => Math.max(max, item.max_working_set_bytes || 0), 0);

        renderSummary("resource-profiles-summary", [
            metricCard("Scenarios", summary.scenario_count ?? scenarios.length),
            metricCard("Allocation probes", summary.allocation_scenarios ?? "-"),
            metricCard("Memory probes", summary.memory_scenarios ?? "-"),
            metricCard("Session probes", summary.session_scenarios ?? "-"),
            metricCard("Worst elapsed", worstElapsed ? `${formatNumber.format(worstElapsed)} ms` : "-"),
            metricCard("Peak allocation", maxAllocated ? formatBytes(maxAllocated) : "-"),
            metricCard("Peak working set", maxWorkingSet ? formatBytes(maxWorkingSet) : "-"),
        ]);

        renderTable(
            "resource-profiles-table",
            ["Scenario", "Focus", "Family", "Samples", "Max elapsed", "Allocated", "Peak live", "Working set", "PF growth", "Handle growth"],
            filteredScenarios.map((item) => `<tr>
                <td><code>${escapeHtml(item.scenario_label || item.scenario)}</code></td>
                <td><span class="pill">${escapeHtml(item.focus || "resource")}</span></td>
                <td><span class="pill">${escapeHtml(item.workload_family || "unmapped")}</span></td>
                <td>${escapeHtml(item.sample_count ?? "-")}</td>
                <td>${formatNumber.format(item.max_elapsed_ms || 0)} ms</td>
                <td>${escapeHtml(formatBytes(item.max_allocated_bytes))}</td>
                <td>${escapeHtml(formatBytes(item.max_peak_live_bytes))}</td>
                <td>${escapeHtml(formatBytes(item.max_working_set_bytes))}</td>
                <td>${item.page_fault_growth == null ? "-" : formatNumber.format(item.page_fault_growth)}</td>
                <td>${item.handle_growth == null ? "-" : formatNumber.format(item.handle_growth)}</td>
            </tr>`)
        );

        renderTable(
            "resource-profiles-samples",
            ["Scenario", "Workload", "Elapsed", "Allocated", "Peak live", "Working set", "Page faults", "Handles", "Result", "Status"],
            sampleRows.map((item) => `<tr>
                <td><code>${escapeHtml(item.scenarioLabel)}</code><div class="muted">${escapeHtml(item.focus)} • ${escapeHtml(item.workloadFamily)}</div></td>
                <td>${escapeHtml(item.workload_label || "-")}</td>
                <td>${formatNumber.format(item.elapsed_ms || 0)} ms</td>
                <td>${escapeHtml(formatBytes(item.allocated_bytes))}<div class="muted">${formatNumber.format(item.allocation_count || 0)} allocs / ${formatNumber.format(item.reallocation_count || 0)} reallocs</div></td>
                <td>${escapeHtml(formatBytes(item.peak_live_bytes))}</td>
                <td>${escapeHtml(formatBytes(item.working_set_bytes))}</td>
                <td>${item.page_fault_count == null ? "-" : formatNumber.format(item.page_fault_count)}</td>
                <td>${item.handle_count == null ? "-" : formatNumber.format(item.handle_count)}</td>
                <td>${escapeHtml(item.result_label || "-")}</td>
                <td class="${item.status === "ok" ? "risk-good" : "risk-bad"}">${escapeHtml(item.status || "-")}${item.note ? `<div class="muted">${escapeHtml(item.note)}</div>` : ""}</td>
            </tr>`)
        );
    }

    function renderChartLegend(series) {
        return series.map((entry) => `<span class="chart-legend__item">
                <svg class="chart-legend__swatch" viewBox="0 0 28 10" aria-hidden="true">
                    <line x1="1" y1="5" x2="27" y2="5" stroke="${entry.color}" stroke-width="3"${entry.dasharray ? ` stroke-dasharray="${entry.dasharray}"` : ""}></line>
                </svg>
                <span>${escapeHtml(entry.label)}</span>
            </span>`).join("");
    }

    function buildLogTicks(min, max) {
        const safeMin = Math.max(min / 1.15, 0.001);
        const safeMax = max * 1.15;
        const ticks = [];
        for (let exponent = Math.floor(Math.log10(safeMin)); exponent <= Math.ceil(Math.log10(safeMax)); exponent += 1) {
            [1, 2, 5].forEach((factor) => {
                const tick = factor * 10 ** exponent;
                if (tick >= safeMin && tick <= safeMax) {
                    ticks.push(tick);
                }
            });
        }
        return ticks.length ? ticks : [safeMin, safeMax];
    }

    function buildLinearTicks(max) {
        const step = max <= 2 ? 0.5 : max <= 4 ? 1 : 2;
        const ticks = [];
        for (let value = 0; value <= max + 0.0001; value += step) {
            ticks.push(Number(value.toFixed(2)));
        }
        return ticks;
    }

    function calculateDoublingMultiplier(points) {
        if (!points || points.length < 2) {
            return null;
        }

        const exponents = [];
        for (let index = 1; index < points.length; index += 1) {
            const previous = points[index - 1];
            const current = points[index];
            const xRatio = current.xValue / previous.xValue;
            const yRatio = current.meanMs / previous.meanMs;
            if (xRatio > 1 && yRatio > 0) {
                exponents.push(Math.log2(yRatio) / Math.log2(xRatio));
            }
        }

        return exponents.length ? 2 ** mean(exponents) : null;
    }

    function mean(values) {
        if (!values.length) {
            return null;
        }
        return values.reduce((sum, value) => sum + value, 0) / values.length;
    }

    function describeGrowth(multiplier) {
        if (multiplier < 1.2) {
            return `nearly flat (${formatNumber.format(multiplier)}x time per 2x growth)`;
        }
        if (multiplier < 1.8) {
            return `sub-linear (${formatNumber.format(multiplier)}x time per 2x growth)`;
        }
        if (multiplier < 2.2) {
            return `roughly linear (${formatNumber.format(multiplier)}x time per 2x growth)`;
        }
        return `super-linear (${formatNumber.format(multiplier)}x time per 2x growth)`;
    }

    function latencyLabel(value) {
        return value === "first_response" ? "First response" : "Completion";
    }

    function titleCase(value) {
        return String(value || "")
            .split(/[_\s-]+/)
            .filter(Boolean)
            .map((item) => item.charAt(0).toUpperCase() + item.slice(1))
            .join(" ");
    }

    function formatAxisMs(value) {
        return value >= 10 ? formatNumber.format(value) : formatNumber.format(Number(value.toFixed(2)));
    }

    function formatBytes(value) {
        if (value == null || !Number.isFinite(value)) {
            return "-";
        }
        if (value >= 1024 * 1024) {
            return `${formatNumber.format(value / (1024 * 1024))} MB`;
        }
        if (value >= 1024) {
            return `${formatNumber.format(value / 1024)} KB`;
        }
        return `${formatNumber.format(value)} B`;
    }

    function pillValues(value) {
        return Array.isArray(value)
            ? value
            : String(value || "")
                .split(",")
                .map((item) => item.trim())
                .filter(Boolean);
    }

    function renderPills(value) {
        const values = pillValues(value);
        if (!values.length) {
            return '<span class="muted">-</span>';
        }
        return values.map((item) => `<span class="pill">${escapeHtml(item)}</span>`).join("");
    }

    function renderOverview() {
        renderHealthGauges();
        renderRiskTreemap();
        renderTopConcerns();
        renderRunStrip();
    }

    function classifyStatus(level) {
        if (level === "stale") return { label: "Stale", cls: "stale" };
        if (level === "bad") return { label: "Regressed", cls: "bad" };
        if (level === "watch") return { label: "Watch", cls: "watch" };
        return { label: "OK", cls: "ok" };
    }

    function renderGaugeCard({ id, title, value, status, driver, sparkline, deltaInfo }) {
        const s = classifyStatus(status);
        const delta = deltaInfo
            ? `<span class="gauge-card__delta gauge-card__delta--${deltaInfo.direction}">${escapeHtml(deltaInfo.label)}</span>`
            : `<span class="gauge-card__delta">no history</span>`;
        const spark = sparkline && sparkline.length >= 2 ? renderSparkline(sparkline) : "";
        return `<div class="gauge-card" id="${id}">
            <div class="gauge-card__title">
                <span>${escapeHtml(title)}</span>
                <span class="gauge-card__status gauge-card__status--${s.cls}">${s.label}</span>
            </div>
            <div class="gauge-card__metric">
                <span class="gauge-card__value">${escapeHtml(value)}</span>
                ${delta}
            </div>
            <div class="gauge-card__driver">${escapeHtml(driver || "")}</div>
            ${spark}
        </div>`;
    }

    function renderSparkline(values) {
        if (!values.length) return "";
        const w = 200, h = 44, pad = 2;
        const min = Math.min(...values);
        const max = Math.max(...values);
        const range = max - min || 1;
        const points = values.map((v, i) => {
            const x = pad + (i * (w - pad * 2)) / Math.max(1, values.length - 1);
            const y = h - pad - ((v - min) / range) * (h - pad * 2);
            return `${x.toFixed(1)},${y.toFixed(1)}`;
        }).join(" ");
        const last = values[values.length - 1];
        const lastX = w - pad;
        const lastY = h - pad - ((last - min) / range) * (h - pad * 2);
        return `<svg class="gauge-card__sparkline" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">
            <polyline fill="none" stroke="#6fd0ff" stroke-width="2" points="${points}" />
            <circle cx="${lastX}" cy="${lastY.toFixed(1)}" r="2.5" fill="#6fd0ff" />
        </svg>`;
    }

    function runMetricSeries(metricKey) {
        // Pull headline metric values from finished runs in chronological order.
        return state.runs
            .filter((run) => run.metrics && run.metrics[metricKey] != null && run.finished_at)
            .map((run) => Number(run.metrics[metricKey]))
            .filter((v) => Number.isFinite(v))
            .slice(-12);
    }

    function describeDelta(series, { higherIsBetter = false } = {}) {
        if (!series || series.length < 2) return null;
        const last = series[series.length - 1];
        const prev = series[series.length - 2];
        if (prev === 0 && last === 0) return null;
        const diff = last - prev;
        const pct = prev === 0 ? null : (diff / Math.abs(prev)) * 100;
        const arrow = diff === 0 ? "" : diff > 0 ? "▲" : "▼";
        const direction = diff === 0 ? "flat" : (diff > 0 ? (higherIsBetter ? "down" : "up") : (higherIsBetter ? "up" : "down"));
        const label = pct == null ? `${arrow} ${formatNumber.format(diff)}` : `${arrow} ${formatNumber.format(Math.abs(pct))}%`;
        return { direction, label };
    }

    function computeQualityHealth() {
        const hotspots = state.hotspots || [];
        const clones = state.clones || [];
        const bad = hotspots.filter((h) => qualityScore(h) >= 600).length;
        const warn = hotspots.filter((h) => {
            const s = qualityScore(h);
            return s >= 300 && s < 600;
        }).length;
        const cloneRisks = clones.filter((c) => (c.score || 0) >= 40).length;
        const total = bad + warn + cloneRisks;
        let status = "ok";
        if (bad > 0 || cloneRisks > 0) status = "bad";
        else if (warn > 0) status = "watch";
        if (!hotspots.length && !clones.length) status = "stale";
        const worst = hotspots[0];
        const driver = worst
            ? `Worst: ${worst.name.split(/[\\/]/).pop()} (${formatNumber.format(qualityScore(worst))})`
            : "No hotspots data";
        return {
            status,
            value: String(total),
            driver,
            series: runMetricSeries("quality_risk_count"),
        };
    }

    function computeCapacityHealth() {
        const speed = state.speedReport || {};
        const summary = speed.summary || {};
        const triageSummary = speed.triage_summary || null;
        const overBudget = summary.over_budget_latency ?? 0;
        const coverageGaps = summary.coverage_gaps ?? 0;
        const ceilings = summary.near_failure_ceilings ?? 0;
        let status = "ok";
        let value = "0";
        let driver = "All scenarios within budget";
        if (triageSummary) {
            const critical = triageSummary.critical ?? 0;
            const watch = triageSummary.watch ?? 0;
            value = String(critical + watch);
            if (critical > 0) { status = "bad"; driver = `${critical} critical, ${watch} to watch`; }
            else if (watch > 0) { status = "watch"; driver = `${watch} scenarios approaching budget`; }
        } else {
            const total = overBudget + ceilings;
            value = String(total);
            if (overBudget > 0 || ceilings > 0) {
                status = overBudget > 2 || ceilings > 0 ? "bad" : "watch";
                driver = `${overBudget} over budget, ${ceilings} near ceiling, ${coverageGaps} coverage gaps`;
            }
        }
        if (!state.speedReport) { status = "stale"; value = "—"; driver = "Run performance refresh"; }
        return {
            status,
            value,
            driver,
            series: runMetricSeries("capacity_risk_count"),
        };
    }

    function computeCorrectnessHealth() {
        const c = state.correctness || {};
        const summary = c.summary || {};
        const failed = summary.failed ?? 0;
        const unknown = summary.unknown ?? 0;
        const total = summary.test_count ?? 0;
        let status = "ok";
        let driver = `${total} tests, all passing`;
        if (failed > 0) { status = "bad"; driver = `${failed} failed, ${unknown} unknown`; }
        else if (unknown > 0) { status = "watch"; driver = `${unknown} tests have not been run`; }
        if (!state.correctness) { status = "stale"; driver = "Run correctness refresh"; }
        const value = state.correctness ? `${total - failed - unknown}/${total}` : "—";
        return {
            status,
            value,
            driver,
            series: runMetricSeries("tests_passed"),
        };
    }

    function renderHealthGauges() {
        const target = byId("overview-health");
        if (!target) return;
        const quality = computeQualityHealth();
        const capacity = computeCapacityHealth();
        const correctness = computeCorrectnessHealth();
        target.innerHTML = [
            renderGaugeCard({
                id: "gauge-quality",
                title: "Quality",
                value: quality.value,
                status: quality.status,
                driver: quality.driver,
                sparkline: quality.series,
                deltaInfo: describeDelta(quality.series),
            }),
            renderGaugeCard({
                id: "gauge-capacity",
                title: "Capacity",
                value: capacity.value,
                status: capacity.status,
                driver: capacity.driver,
                sparkline: capacity.series,
                deltaInfo: describeDelta(capacity.series),
            }),
            renderGaugeCard({
                id: "gauge-correctness",
                title: "Correctness",
                value: correctness.value,
                status: correctness.status,
                driver: correctness.driver,
                sparkline: correctness.series,
                deltaInfo: describeDelta(correctness.series, { higherIsBetter: true }),
            }),
        ].join("");
    }

    function moduleScoreFor(module, metric) {
        const m = module.metrics || {};
        if (metric === "total_score") return Number(module.total_score ?? m.total_score ?? 0);
        return Number(module[metric] ?? m[metric] ?? 0);
    }

    function moduleSloc(module) {
        const m = module.metrics || {};
        return Number(module.sloc ?? module.size ?? m.sloc ?? m.size ?? 1);
    }

    function moduleSignals(module) {
        const m = module.metrics || {};
        if (Array.isArray(module.signals)) return module.signals;
        if (Array.isArray(m.signals)) return m.signals;
        return [];
    }

    function moduleRiskDrivers(module) {
        return riskMetricLabels
            .map(([key, label]) => ({ label, score: moduleScoreFor(module, key) }))
            .filter((item) => item.score > 0)
            .sort((a, b) => b.score - a.score)
            .slice(0, 3);
    }

    function riskTooltip(module, score) {
        const signals = moduleSignals(module).slice(0, 3);
        const drivers = moduleRiskDrivers(module)
            .map((item) => `${item.label} ${formatNumber.format(item.score)}`);
        return [
            module.id || module.name || "Unknown module",
            `Total risk ${formatNumber.format(score)}`,
            drivers.length ? `Top drivers: ${drivers.join(", ")}` : "",
            signals.length ? `Signals: ${signals.join("; ")}` : "",
            `SLOC ${formatNumber.format(moduleSloc(module))}`,
        ].filter(Boolean).join("\n");
    }

    function colorForScore(score, max) {
        const t = max ? Math.min(1, score / max) : 0;
        // Interpolate good -> warn -> bad.
        if (t < 0.5) {
            const k = t / 0.5;
            return mixColor([125, 220, 155], [243, 201, 105], k);
        }
        const k = (t - 0.5) / 0.5;
        return mixColor([243, 201, 105], [255, 116, 116], k);
    }

    function mixColor(a, b, t) {
        const r = Math.round(a[0] + (b[0] - a[0]) * t);
        const g = Math.round(a[1] + (b[1] - a[1]) * t);
        const bl = Math.round(a[2] + (b[2] - a[2]) * t);
        return `rgb(${r}, ${g}, ${bl})`;
    }

    function renderRiskTreemap() {
        const target = byId("overview-treemap");
        if (!target) return;
        const graph = state.map?.graph;
        if (!graph || !graph.nodes) {
            target.innerHTML = `<div class="risk-treemap__empty">No map data. Run the Map refresh.</div>`;
            return;
        }
        const modules = graph.nodes
            .map((n) => n.data)
            .filter((n) => n && !n.is_group);
        if (!modules.length) {
            target.innerHTML = `<div class="risk-treemap__empty">No modules to display.</div>`;
            return;
        }
        const filtered = modules
            .map((m) => ({ module: m, score: moduleScoreFor(m, "total_score") }))
            .filter(({ module, score }) => {
                if (state.overviewRiskFilter === "high") return score >= 600;
                if (state.overviewRiskFilter === "with-signals") return moduleSignals(module).length > 0;
                return true;
            })
            .sort((a, b) => b.score - a.score);
        const ranked = state.overviewRiskMode === "top" ? filtered.slice(0, 24) : filtered;
        if (!ranked.length) {
            target.innerHTML = `<div class="risk-treemap__empty">No modules match the selected risk filters.</div>`;
            return;
        }
        const maxScore = ranked[0]?.score || 1;
        const scopeLabel = state.overviewRiskMode === "top" ? "Top risk" : "All modules";
        const filterLabel = state.overviewRiskFilter === "high" ? "high risk only"
            : state.overviewRiskFilter === "with-signals" ? "with signals"
                : "all risk levels";
        target.innerHTML = `<div class="risk-treemap__meta">${escapeHtml(scopeLabel)}: showing ${ranked.length} of ${modules.length} modules, ${escapeHtml(filterLabel)}. Highest score ${formatNumber.format(maxScore)}.</div>` + ranked.map(({ module, score }) => {
            const sloc = moduleSloc(module);
            const widthBasis = Math.max(80, Math.min(260, 60 + Math.sqrt(sloc) * 4));
            const color = colorForScore(score, maxScore);
            const label = (module.name || module.id || "?").split(/[\\/]/).slice(-2).join("/");
            const drivers = moduleRiskDrivers(module).map((item) => item.label).join(", ");
            return `<div class="risk-treemap__cell" style="flex: 1 1 ${widthBasis}px; background:${color};" title="${escapeHtml(riskTooltip(module, score))}" data-module-id="${escapeHtml(module.id)}">
                <span class="risk-treemap__label">${escapeHtml(label)}</span>
                <span class="risk-treemap__score">${formatNumber.format(score)}</span>
                ${drivers ? `<span class="risk-treemap__drivers">${escapeHtml(drivers)}</span>` : ""}
            </div>`;
        }).join("");
        target.querySelectorAll(".risk-treemap__cell").forEach((el) => {
            el.addEventListener("click", () => {
                state.selectedModule = el.dataset.moduleId;
                document.querySelector('.tab[data-tab="map"]')?.click();
                renderMap();
            });
        });
    }

    function renderTopListCard({ title, subtitle, items, emptyText }) {
        const list = items.length
            ? `<ol>${items.map((item) => `<li><span class="top-list-card__label">${item.label}</span><span class="top-list-card__value">${item.value}</span></li>`).join("")}</ol>`
            : `<p class="muted">${escapeHtml(emptyText)}</p>`;
        return `<div class="top-list-card">
            <h3>${escapeHtml(title)}</h3>
            <p>${escapeHtml(subtitle)}</p>
            ${list}
        </div>`;
    }

    function renderTopConcerns() {
        const target = byId("overview-top-concerns");
        if (!target) return;

        const qualityItems = [...(state.hotspots || [])]
            .sort((a, b) => qualityScore(b) - qualityScore(a))
            .slice(0, 5)
            .map((item) => ({
                label: `<code>${escapeHtml((item.name || "").split(/[\\/]/).pop() || item.name)}</code>`,
                value: `<span class="${riskClass(qualityScore(item), 300, 600)}">${formatNumber.format(qualityScore(item))}</span>`,
            }));

        const slowItems = [...(state.slowspots || []), ...(state.searchSpeed || [])]
            .map((item) => ({
                name: item.scenario_label || item.name,
                ratio: item.threshold_ms ? (item.mean_ns / 1_000_000) / item.threshold_ms : 0,
            }))
            .filter((it) => it.ratio > 0)
            .sort((a, b) => b.ratio - a.ratio)
            .slice(0, 5)
            .map((it) => ({
                label: `<code>${escapeHtml((it.name || "").split(/[\\/]/).pop() || it.name)}</code>`,
                value: `<span class="${it.ratio > 1 ? "risk-bad" : it.ratio > 0.85 ? "risk-warn" : "risk-good"}">${formatNumber.format(it.ratio * 100)}%</span>`,
            }));

        const tests = state.correctness?.tests || [];
        const testItems = tests
            .filter((t) => t.last_status === "failed" || t.last_status === "unknown")
            .slice(0, 5)
            .map((t) => ({
                label: `<code>${escapeHtml(t.name || t.path)}</code>`,
                value: `<span class="${t.last_status === "failed" ? "risk-bad" : "risk-warn"}">${escapeHtml(t.last_status)}</span>`,
            }));

        target.innerHTML = [
            renderTopListCard({
                title: "Top quality risks",
                subtitle: "Highest hotspot scores.",
                items: qualityItems,
                emptyText: "No hotspot data.",
            }),
            renderTopListCard({
                title: "Slowest vs budget",
                subtitle: "Mean latency relative to threshold.",
                items: slowItems,
                emptyText: "No benchmark data.",
            }),
            renderTopListCard({
                title: "Failing or unknown tests",
                subtitle: "Need attention or have not run.",
                items: testItems,
                emptyText: state.correctness ? "All tests passing." : "No test data.",
            }),
        ].join("");
    }

    function renderRunStrip() {
        const target = byId("overview-run-strip");
        if (!target) return;
        const runs = [...state.runs].slice(-12).reverse();
        if (!runs.length) {
            target.innerHTML = `<p class="muted">No runs yet. Use the Refresh buttons above to start one.</p>`;
            return;
        }
        target.innerHTML = runs.map((run) => {
            const status = (run.status || "queued").toLowerCase();
            const dur = run.duration_seconds == null ? "" : ` ${formatNumber.format(run.duration_seconds)}s`;
            const ts = run.finished_at || run.started_at || run.created_at || 0;
            const tsLabel = ts ? new Date(ts * 1000).toLocaleTimeString() : run.id;
            const isActive = state.selectedRun === run.id;
            return `<button type="button" class="run-strip__dot ${isActive ? "is-active" : ""}" data-run-id="${escapeHtml(run.id)}" title="${escapeHtml(run.selector || run.id)}">
                <span class="run-strip__bullet run-strip__bullet--${status}"></span>
                <span>${escapeHtml(tsLabel)}${escapeHtml(dur)}</span>
            </button>`;
        }).join("");
        target.querySelectorAll(".run-strip__dot").forEach((el) => {
            el.addEventListener("click", () => {
                const id = el.dataset.runId;
                state.selectedRun = id;
                const out = byId("overview-run-log");
                if (out) {
                    out.hidden = false;
                    out.textContent = "Loading run log...";
                }
                loadRunLog(id, "overview-run-log");
                renderRunStrip();
            });
        });
    }

    function renderPerformanceScenarios() {
        const scenarios = [
            {
                title: "Large Files: Loading And Manipulating",
                description: "Open, scroll, viewport, snapshot, and large-document edit evidence.",
                families: ["large-file-load", "scroll"],
                profiles: ["large_file_scroll", "viewport_extraction", "document_snapshot"],
            },
            {
                title: "Large Amount Of Tabs: Loading And Manipulating",
                description: "Tab count, switching, reordering, and tab-strip manipulation.",
                families: ["tab-management"],
                profiles: ["tab_operations", "tab_tile_layout"],
            },
            {
                title: "Cutting/Pasting: Large Amounts Of Data",
                description: "Large paste, cut, undo, redo, and metadata refresh costs.",
                families: ["edit-paste"],
                profiles: ["large_file_paste"],
            },
            {
                title: "Splitting: Large Amount Of Tabs",
                description: "Split creation, rebalance, close, promote, and restore costs.",
                families: ["split-layout"],
                profiles: ["large_file_split", "tab_tile_layout"],
            },
            {
                title: "Session Persistence Restore",
                description: "Persist and restore cost for large saved workspaces.",
                families: ["session"],
                profiles: [],
                tests: ["tests/session_store_tests.rs", "tests/startup_tests.rs"],
            },
            {
                title: "Searching: Large Files And Lots Of Files",
                description: "Active, current, and all-tab search scaling.",
                families: ["search", "search-dispatch"],
                profiles: ["search_current_app_state", "search_all_tabs", "search_dispatch"],
                tests: ["tests/search_tests.rs"],
            },
        ];
        const slowspots = state.slowspots || [];
        const capacity = state.capacityReport?.scenarios || [];
        const resources = state.resourceProfiles?.scenarios || [];
        const flamegraphs = state.flamegraphs || [];
        const scenarioStats = scenarios.map((scenario) => {
            const familySet = new Set(scenario.families);
            const speedCount = slowspots.filter((item) => familySet.has(item.workload_family)).length;
            const capacityCount = capacity.filter((item) => familySet.has(item.workload_family)).length;
            const resourceCount = resources.filter((item) => familySet.has(item.workload_family)).length;
            const profileMatches = flamegraphs.filter((item) => scenario.profiles.includes(item.id) || (item.workload_families || []).some((family) => familySet.has(family)));
            return {
                ...scenario,
                speedCount,
                capacityCount,
                resourceCount,
                profileAvailable: profileMatches.filter((item) => item.available).length,
                profileTotal: profileMatches.length,
            };
        });
        const maxEvidence = Math.max(1, ...scenarioStats.flatMap((scenario) => [
            scenario.speedCount,
            scenario.capacityCount,
            scenario.resourceCount,
        ]));
        byId("performance-scenarios").innerHTML = scenarioStats.map((scenario) => {
            const rows = [
                { label: "Speed", value: scenario.speedCount, cls: "speed" },
                { label: "Capacity", value: scenario.capacityCount, cls: "capacity" },
                { label: "Resource", value: scenario.resourceCount, cls: "resource" },
            ];
            const profilePct = scenario.profileTotal ? (scenario.profileAvailable / scenario.profileTotal) * 100 : 0;
            return `<article class="scenario-card">
                <h3>${escapeHtml(scenario.title)}</h3>
                <div class="scenario-graphs">
                    <div class="scenario-bars">
                        ${rows.map((row) => `<div class="scenario-bars__row">
                            <span>${escapeHtml(row.label)}</span>
                            <div class="scenario-bars__track">
                                <span class="scenario-bars__fill scenario-bars__fill--${row.cls}" style="width:${Math.max(row.value ? 4 : 0, (row.value / maxEvidence) * 100)}%"></span>
                            </div>
                            <strong>${formatNumber.format(row.value)}</strong>
                        </div>`).join("")}
                    </div>
                    <div class="scenario-coverage">
                        <div class="scenario-coverage__top">
                            <span>Flamegraphs</span>
                            <strong>${formatNumber.format(scenario.profileAvailable)}/${formatNumber.format(scenario.profileTotal)}</strong>
                        </div>
                        <div class="scenario-coverage__track">
                            <span style="width:${profilePct}%"></span>
                        </div>
                        ${scenario.tests?.length ? `<div class="scenario-tests">${renderPills(scenario.tests)}</div>` : ""}
                    </div>
                </div>
            </article>`;
        }).join("");
    }

    function renderCorrectness() {
        const payload = state.correctness || {};
        const summary = payload.summary || {};
        const layers = payload.layers || [];
        const tests = payload.tests || [];
        const query = byId("correctness-filter")?.value || "";
        const showAll = byId("correctness-show-all")?.checked ?? false;
        const layerFilter = state.selectedLayer;
        let filtered = tests.filter((item) => matchesFilter(item, query));
        if (layerFilter) {
            filtered = filtered.filter((item) => item.layer === layerFilter);
        }
        if (!showAll) {
            filtered = filtered.filter((item) => item.last_status === "failed" || item.last_status === "unknown");
        }
        renderSummary("correctness-summary", [
            metricCard("Tests", summary.test_count ?? tests.length),
            metricCard("Integration", summary.integration_count ?? "-"),
            metricCard("Inline", summary.inline_count ?? "-"),
            metricCard("Layers", summary.layers ?? layers.length),
            metricCard("Failed", summary.failed ?? "-"),
            metricCard("Unknown", summary.unknown ?? "-"),
        ]);
        renderTable(
            "correctness-layers",
            ["Layer", "Total", "Passed", "Failed", "Skipped", "Unknown"],
            layers.map((item) => `<tr>
                <td>${escapeHtml(item.name)}</td>
                <td>${formatNumber.format(item.total || 0)}</td>
                <td class="risk-good">${formatNumber.format(item.passed || 0)}</td>
                <td class="${item.failed ? "risk-bad" : "risk-good"}">${formatNumber.format(item.failed || 0)}</td>
                <td>${formatNumber.format(item.skipped || 0)}</td>
                <td>${formatNumber.format(item.unknown || 0)}</td>
            </tr>`)
        );
        renderTable(
            "correctness-table",
            ["Layer", "Test", "Description", "Kind", "Status", "Command"],
            filtered.map((item) => `<tr>
                <td><span class="pill">${escapeHtml(item.layer)}</span></td>
                <td><code>${escapeHtml(item.path)}:${escapeHtml(item.line)}</code><div class="muted">${escapeHtml(item.name)}</div></td>
                <td>${escapeHtml(item.description)}</td>
                <td><span class="pill">${escapeHtml(item.kind)}</span></td>
                <td class="${item.last_status === "failed" ? "risk-bad" : item.last_status === "passed" ? "risk-good" : "risk-warn"}">${escapeHtml(item.last_status)}</td>
                <td><code>${escapeHtml(item.command)}</code></td>
            </tr>`)
        );
    }

    function renderRunLog() {
        const runs = [...state.runs].reverse();
        const running = runs.filter((item) => item.status === "running" || item.status === "queued").length;
        const failed = runs.filter((item) => item.status === "failed").length;
        const activeRun = runs.find((item) => item.status === "running" || item.status === "queued");
        renderSummary("run-log-summary", [
            metricCard("Runs", runs.length),
            metricCard("Running", running),
            metricCard("Failed", failed),
            metricCard("Latest", runs[0]?.status || "-"),
            metricCard("Active Progress", activeRun ? runProgressLabel(activeRun) : "-"),
        ]);
        renderTable(
            "run-log-table",
            ["Run", "Selector", "Tasks", "Progress", "Status", "Duration", "Artifacts"],
            runs.map((item) => `<tr class="run-row" data-run-id="${escapeHtml(item.id)}">
                <td><code>${escapeHtml(item.id)}</code></td>
                <td>${escapeHtml(item.selector || "-")}</td>
                <td>${renderPills(item.task_ids || [])}</td>
                <td>${renderRunProgress(item, "table")}</td>
                <td><span class="pill">${escapeHtml(item.status || "-")}</span></td>
                <td>${item.duration_seconds == null ? "-" : `${formatNumber.format(item.duration_seconds)} s`}</td>
                <td>${renderPills(item.artifacts || [])}</td>
            </tr>`)
        );
        byId("run-log-table").querySelectorAll(".run-row").forEach((row) => {
            row.addEventListener("click", () => loadRunLog(row.dataset.runId));
        });
        renderSelectedRunProgress();
    }

    function runProgress(run) {
        const taskIds = run.task_ids || [];
        const total = Number(run.total_tasks ?? taskIds.length ?? 0);
        let done = Number(run.completed_tasks ?? 0);
        if (!Number.isFinite(done)) done = 0;
        if (run.status === "completed" && total > 0) done = total;
        done = Math.max(0, Math.min(done, total));
        const left = Math.max(0, total - done);
        const percent = total > 0 ? Math.round((done / total) * 100) : 0;
        return { total, done, left, percent };
    }

    function runProgressLabel(run) {
        const progress = runProgress(run);
        if (!progress.total) return "-";
        const current = run.current_task_id ? ` · ${run.current_task_id}` : "";
        return `${progress.done}/${progress.total}${current}`;
    }

    function renderRunProgress(run, density = "detail") {
        const progress = runProgress(run);
        if (!progress.total) return '<span class="muted">-</span>';
        const statusClass = run.status === "failed"
            ? "is-failed"
            : run.status === "completed"
                ? "is-complete"
                : run.status === "interrupted"
                    ? "is-interrupted"
                    : "is-running";
        const current = run.current_task_id
            ? `<span class="run-progress__current">${escapeHtml(run.current_task_id)}</span>`
            : "";
        return `<div class="run-progress run-progress--${density} ${statusClass}">
            <div class="run-progress__track" role="progressbar" aria-valuenow="${progress.percent}" aria-valuemin="0" aria-valuemax="100" aria-label="${escapeHtml(run.id)} progress">
                <div class="run-progress__bar" style="width:${progress.percent}%"></div>
            </div>
            <div class="run-progress__meta">
                <span>${progress.done} done</span>
                <span>${progress.left} left</span>
                <span>${progress.percent}%</span>
            </div>
            ${current}
        </div>`;
    }

    function renderSelectedRunProgress() {
        const target = byId("run-log-progress");
        if (!target) return;
        const run = state.runs.find((item) => item.id === state.selectedRun);
        target.innerHTML = run ? renderRunProgress(run, "detail") : "";
    }

    function renderMap() {
        const payload = state.map;
        if (!payload?.graph) {
            renderSummary("map-summary", [
                metricCard("Nodes", "-"),
                metricCard("Edges", "-"),
                metricCard("High maintainability", "-"),
                metricCard("Untested risk", "-"),
            ]);
            byId("map-graph").innerHTML = '<p class="muted" style="padding: 20px;">No map data loaded.</p>';
            return;
        }

        const query = byId("map-filter").value.toLowerCase();
        const graph = payload.graph;
        let modules = graph.nodes
            .map((node) => node.data)
            .filter((node) => !node.is_group)
            .filter((node) => !query || node.id.toLowerCase().includes(query));

        if (state.focusMode && state.selectedModule) {
            const focusIds = new Set([state.selectedModule]);
            graph.edges.forEach((edge) => {
                if (edge.data.source === state.selectedModule) focusIds.add(edge.data.target);
                if (edge.data.target === state.selectedModule) focusIds.add(edge.data.source);
            });
            modules = modules.filter((node) => focusIds.has(node.id));
        }

        const moduleIds = new Set(modules.map((node) => node.id));
        const summary = payload.meta?.summary || {};
        const highMaintainability = modules.filter((node) => (node.maintainability_risk || 0) >= 350).length;
        const lowTestEvidence = modules.filter((node) => !node.evidence?.has_tests).length;
        const visibleEdges = graph.edges
            .map((edge) => edge.data)
            .filter((edge) => moduleIds.has(edge.source) && moduleIds.has(edge.target));

        renderSummary("map-summary", [
            metricCard("Nodes", modules.length),
            metricCard("Edges", visibleEdges.length),
            metricCard("High maintainability", highMaintainability),
            metricCard("Untested risk", lowTestEvidence),
            metricCard("Cycle members", summary.cycle_members ?? "-"),
            metricCard("Selected", state.selectedModule || "-"),
        ]);

        const layout = buildMapLayout(modules);
        const rowMarkup = renderFolderRows(layout);
        const edgeMarkup = visibleEdges.map((edge) => renderEdge(edge, layout)).join("");
        const nodeMarkup = modules.map((node) => renderNode(node, layout)).join("");
        const width = Math.max(1200, layout.width);
        const height = Math.max(720, layout.height);
        const displayWidth = Math.round(width * state.mapZoom);
        const displayHeight = Math.round(height * state.mapZoom);

        byId("map-graph").classList.toggle("has-selection", Boolean(state.selectedModule));
        byId("map-graph").innerHTML = `<svg class="map-svg" width="${displayWidth}" height="${displayHeight}" viewBox="0 0 ${width} ${height}" role="img" aria-label="Architecture dependency map">
            <defs>
                <marker id="arrow-muted" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="rgba(159, 176, 195, 0.35)"></path>
                </marker>
                <marker id="arrow-outbound" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="#7ddc9b"></path>
                </marker>
                <marker id="arrow-inbound" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="#ff7474"></path>
                </marker>
            </defs>
            <g>${rowMarkup}</g>
            <g>${edgeMarkup}</g>
            <g>${nodeMarkup}</g>
        </svg>`;

        byId("map-graph").querySelectorAll(".map-node").forEach((node) => {
            node.addEventListener("click", () => {
                const nodeId = node.getAttribute("data-id");
                state.selectedModule = state.selectedModule === nodeId ? null : nodeId;
                renderMap();
            });
        });

        renderMapDetail(modules, visibleEdges);
    }

    function buildMapLayout(nodes) {
        const groups = new Map();
        const groupNames = new Set();

        if (state.mapLayout === 'layer') {
            nodes.forEach((node) => {
                const layer = node.layer || 'default';
                groupNames.add(layer);
                if (!groups.has(layer)) groups.set(layer, []);
                groups.get(layer).push(node);
            });
            const layerOrder = ["chrome", "ui", "services", "domain", "app_state", "default"];
            const orderedNames = Array.from(groupNames).sort((a, b) => {
                const idxA = layerOrder.indexOf(a);
                const idxB = layerOrder.indexOf(b);
                if (idxA !== -1 && idxB !== -1) return idxA - idxB;
                if (idxA !== -1) return -1;
                if (idxB !== -1) return 1;
                return a.localeCompare(b);
            });
            groupNames.clear();
            orderedNames.forEach(n => groupNames.add(n));
        } else {
            groupNames.add("src");
            nodes.forEach((node) => {
                folderAncestors(node.id).forEach((folder) => groupNames.add(folder));
                const folder = folderPathForModule(node.id);
                if (!groups.has(folder)) {
                    groups.set(folder, []);
                }
                groups.get(folder).push(node);
            });
            const orderedFoldersArr = orderedFolders(groupNames);
            groupNames.clear();
            orderedFoldersArr.forEach(n => groupNames.add(n));
        }

        const nodeWidth = 260;
        const nodeHeight = 88;
        const positions = new Map();
        const rows = [];
        let mapWidth = 0;
        let mapHeight = 0;

        if (state.mapLayout === 'layer') {
            const colWidth = nodeWidth + 60;
            const yGap = 20;
            const topOffset = 76;
            const leftOffset = 40;

            let maxModulesInCol = 0;
            const orderedGroups = Array.from(groupNames);
            orderedGroups.forEach((group) => {
                maxModulesInCol = Math.max(maxModulesInCol, (groups.get(group) || []).length);
            });

            const colHeight = topOffset + maxModulesInCol * (nodeHeight + yGap) + 40;
            mapWidth = leftOffset + orderedGroups.length * colWidth + 40;
            mapHeight = colHeight + 60;

            orderedGroups.forEach((group, colIndex) => {
                const modules = groups.get(group) || [];
                const colX = leftOffset + colIndex * colWidth;

                rows.push({
                    isColumn: true,
                    folder: group,
                    x: colX,
                    y: 30,
                    width: colWidth - 20,
                    height: colHeight,
                    label: group,
                    modules: modules,
                });

                modules
                    .sort((left, right) => {
                        const metricRight = state.mapMetric === 'maintainability' ? right.maintainability_risk :
                            state.mapMetric === 'change' ? right.change_risk :
                                state.mapMetric === 'performance' ? right.performance_risk :
                                    state.mapMetric === 'quality' ? right.quality_risk :
                                        state.mapMetric === 'correctness' ? right.correctness_risk :
                                            state.mapMetric === 'architectural' ? right.architectural_risk :
                                                state.mapMetric === 'churn' ? right.churn : right.total_score;
                        const metricLeft = state.mapMetric === 'maintainability' ? left.maintainability_risk :
                            state.mapMetric === 'change' ? left.change_risk :
                                state.mapMetric === 'performance' ? left.performance_risk :
                                    state.mapMetric === 'quality' ? left.quality_risk :
                                        state.mapMetric === 'correctness' ? left.correctness_risk :
                                            state.mapMetric === 'architectural' ? left.architectural_risk :
                                                state.mapMetric === 'churn' ? left.churn : left.total_score;
                        return (metricRight || 0) - (metricLeft || 0);
                    })
                    .forEach((node, moduleIndex) => {
                        positions.set(node.id, {
                            x: colX + 10,
                            y: topOffset + moduleIndex * (nodeHeight + yGap),
                            folder: group,
                            width: nodeWidth,
                            height: nodeHeight,
                        });
                    });
            });
        } else {
            const rowHeight = 134;
            const xGap = 34;
            const topOffset = 76;
            const leftOffset = 300;
            let maxColumns = 1;

            Array.from(groupNames).forEach((group, rowIndex) => {
                const modules = groups.get(group) || [];
                maxColumns = Math.max(maxColumns, modules.length);
                const rowY = topOffset + rowIndex * rowHeight;

                rows.push({
                    isColumn: false,
                    folder: group,
                    y: rowY,
                    height: rowHeight - 18,
                    label: folderLabel(group),
                    modules: modules,
                });

                modules
                    .sort((left, right) => {
                        const metricRight = state.mapMetric === 'maintainability' ? right.maintainability_risk :
                            state.mapMetric === 'change' ? right.change_risk :
                                state.mapMetric === 'performance' ? right.performance_risk :
                                    state.mapMetric === 'quality' ? right.quality_risk :
                                        state.mapMetric === 'correctness' ? right.correctness_risk :
                                            state.mapMetric === 'architectural' ? right.architectural_risk :
                                                state.mapMetric === 'churn' ? right.churn : right.total_score;
                        const metricLeft = state.mapMetric === 'maintainability' ? left.maintainability_risk :
                            state.mapMetric === 'change' ? left.change_risk :
                                state.mapMetric === 'performance' ? left.performance_risk :
                                    state.mapMetric === 'quality' ? left.quality_risk :
                                        state.mapMetric === 'correctness' ? left.correctness_risk :
                                            state.mapMetric === 'architectural' ? left.architectural_risk :
                                                state.mapMetric === 'churn' ? left.churn : left.total_score;
                        return (metricRight || 0) - (metricLeft || 0);
                    })
                    .forEach((node, columnIndex) => {
                        positions.set(node.id, {
                            x: leftOffset + columnIndex * (nodeWidth + xGap),
                            y: rowY + 14,
                            folder: group,
                            width: nodeWidth,
                            height: nodeHeight,
                        });
                    });
            });

            mapWidth = leftOffset + Math.max(maxColumns, 2) * (nodeWidth + xGap) + 80;
            mapHeight = topOffset + rows.length * rowHeight + 70;
        }

        return {
            positions,
            rows,
            width: mapWidth,
            height: mapHeight,
        };
    }

    function folderAncestors(moduleId) {
        const parts = moduleId.split("::");
        const ancestors = ["src"];
        for (let index = 1; index < parts.length; index += 1) {
            ancestors.push(parts.slice(0, index).join("::"));
        }
        return ancestors;
    }

    function folderPathForModule(moduleId) {
        const parts = moduleId.split("::");
        if (parts.length <= 1) {
            return "src";
        }
        return parts.slice(0, -1).join("::");
    }

    function orderedFolders(folderNames) {
        return Array.from(folderNames).sort((left, right) => {
            if (left === "src") {
                return -1;
            }
            if (right === "src") {
                return 1;
            }
            return left.localeCompare(right);
        });
    }

    function folderDepth(folder) {
        if (folder === "src") {
            return 0;
        }
        return folder.split("::").length;
    }

    function folderLabel(folder) {
        if (folder === "src") {
            return "src";
        }
        return `${"  ".repeat(Math.max(0, folderDepth(folder) - 1))}${folder}`;
    }

    function renderFolderRows(layout) {
        return layout.rows
            .map((row, index) => {
                const tone = index % 2 === 0 ? "rgba(255,255,255,0.035)" : "rgba(255,255,255,0.015)";
                if (row.isColumn) {
                    return `<g class="folder-row" transform="translate(${row.x - 10} ${row.y})">
                        <rect width="${row.width}" height="${row.height}" rx="18" fill="${tone}"></rect>
                        <foreignObject x="18" y="20" width="${row.width - 36}" height="76">
                            <div xmlns="http://www.w3.org/1999/xhtml" class="folder-label">
                                <strong>${escapeHtml(row.label)}</strong>
                                <span>${row.modules.length} modules</span>
                            </div>
                        </foreignObject>
                    </g>`;
                } else {
                    const width = Math.max(900, layout.width - 60);
                    return `<g class="folder-row" transform="translate(30 ${row.y - 10})">
                        <rect width="${width}" height="${row.height}" rx="18" fill="${tone}"></rect>
                        <foreignObject x="18" y="20" width="218" height="76">
                            <div xmlns="http://www.w3.org/1999/xhtml" class="folder-label">
                                <strong>${escapeHtml(row.label)}</strong>
                                <span>${row.modules.length} modules</span>
                            </div>
                        </foreignObject>
                    </g>`;
                }
            }).join("");
    }

    function renderEdge(edge, layout) {
        const source = layout.positions.get(edge.source);
        const target = layout.positions.get(edge.target);
        if (!source || !target) {
            return "";
        }

        const selected = state.selectedModule;
        const className = [
            "map-link",
            selected === edge.source ? "is-outbound" : "",
            selected === edge.target ? "is-inbound" : "",
        ].filter(Boolean).join(" ");
        const startX = source.x + source.width / 2;
        const startY = source.y + source.height;
        const endX = target.x + target.width / 2;
        const endY = target.y;
        const midY = startY + (endY - startY) / 2;
        return `<path class="${className}" d="M ${startX} ${startY} C ${startX} ${midY}, ${endX} ${midY}, ${endX} ${endY}" />`;
    }

    function renderNode(node, layout) {
        const position = layout.positions.get(node.id);
        const selected = state.selectedModule;
        const outboundIds = linkedIds(selected, "outbound");
        const inboundIds = linkedIds(selected, "inbound");
        const className = [
            "map-node",
            selected === node.id ? "is-selected" : "",
            outboundIds.has(node.id) ? "is-outbound" : "",
            inboundIds.has(node.id) ? "is-inbound" : "",
        ].filter(Boolean).join(" ");

        const metricValue = mapMetricValue(node);
        const fill = scoreFill(metricValue || 0, state.mapMetric);
        const label = shortenLabel(node.id);
        const score = formatNumber.format(metricValue || 0);
        const chips = [
            `Q ${Math.round(node.quality_risk ?? node.maintainability_risk ?? 0)}`,
            `M ${Math.round(node.maintainability_risk || 0)}`,
            `T ${Math.round(node.correctness_risk || 0)}`,
            `C ${Math.round(node.change_risk || 0)}`,
            `P ${Math.round(node.performance_risk || 0)}`,
            `A ${Math.round(node.architectural_risk || 0)}`,
        ].join(" · ");

        return `<g class="${className}" data-id="${escapeHtml(node.id)}" transform="translate(${position.x} ${position.y})">
            <title>${escapeHtml(node.id)}</title>
            <rect width="${position.width}" height="${position.height}" rx="16" fill="${fill}"></rect>
            <foreignObject x="14" y="12" width="${position.width - 28}" height="${position.height - 24}">
                <div xmlns="http://www.w3.org/1999/xhtml" class="node-label">
                    <strong>${escapeHtml(label)}</strong>
                    <span>${escapeHtml(state.mapMetric)} ${escapeHtml(score)}</span>
                    <span>${escapeHtml(chips)}</span>
                </div>
            </foreignObject>
        </g>`;
    }

    function linkedIds(selected, direction) {
        if (!selected || !state.map?.graph?.edges) {
            return new Set();
        }
        const ids = state.map.graph.edges
            .map((edge) => edge.data)
            .filter((edge) => direction === "outbound" ? edge.source === selected : edge.target === selected)
            .map((edge) => direction === "outbound" ? edge.target : edge.source);
        return new Set(ids);
    }

    function mapMetricValue(node) {
        if (state.mapMetric === 'maintainability') return node.maintainability_risk;
        if (state.mapMetric === 'quality') return node.quality_risk ?? node.maintainability_risk;
        if (state.mapMetric === 'correctness') return node.correctness_risk;
        if (state.mapMetric === 'change') return node.change_risk;
        if (state.mapMetric === 'performance') return node.performance_risk;
        if (state.mapMetric === 'architectural') return node.architectural_risk;
        if (state.mapMetric === 'churn') return node.churn;
        return node.total_score;
    }

    function scoreFill(score, metric) {
        let bad = 600;
        let warn = 300;
        if (metric === 'maintainability' || metric === 'architectural') { bad = 350; warn = 150; }
        else if (metric === 'quality') { bad = 350; warn = 150; }
        else if (metric === 'correctness') { bad = 120; warn = 60; }
        else if (metric === 'change') { bad = 200; warn = 80; }
        else if (metric === 'performance') { bad = 100; warn = 30; }
        else if (metric === 'churn') { bad = 500; warn = 150; }

        if (score >= bad) return "#6b2a35";
        if (score >= warn) return "#5e4b25";
        return "#244638";
    }

    function shortenLabel(id) {
        const parts = id.split("::");
        if (parts.length <= 2) {
            return id;
        }
        return `${parts.at(-2)}::${parts.at(-1)}`;
    }

    function renderMapDetail(modules, edges) {
        const selected = modules.find((node) => node.id === state.selectedModule);
        if (!selected) {
            const getMetric = (node) => mapMetricValue(node);
            const top5 = [...modules].sort((a, b) => (getMetric(b) || 0) - (getMetric(a) || 0)).slice(0, 5);
            const top5Html = top5.map((n, i) => {
                return `<div class="detail-row"><strong>${i + 1}. ${escapeHtml(shortenLabel(n.id))}</strong>${formatNumber.format(getMetric(n) || 0)}</div>`;
            }).join('');

            byId("map-detail").innerHTML = `<h2>Insights</h2>
                <p class="muted" style="margin-bottom: 1rem;">Top 5 modules by <strong>${state.mapMetric}</strong>. Click a module on the map to see details.</p>
                <div class="detail-list">${top5Html}</div>`;
            return;
        }

        const outbound = edges.filter((edge) => edge.source === selected.id).map((edge) => edge.target);
        const inbound = edges.filter((edge) => edge.target === selected.id).map((edge) => edge.source);
        const perf = selected.perf_benchmarks || [];
        const evidence = selected.evidence || {};
        const categorySignals = selected.category_signals || {};

        byId("map-detail").innerHTML = `<h2>${escapeHtml(selected.id)}</h2>
            <div class="detail-list">
                <div class="detail-row"><strong>Total risk</strong>${formatNumber.format(selected.total_score || 0)}</div>
                <div class="detail-row"><strong>Quality risk</strong>${formatNumber.format(selected.quality_risk ?? selected.maintainability_risk ?? 0)}</div>
                <div class="detail-row"><strong>Maintainability risk</strong>${formatNumber.format(selected.maintainability_risk || 0)}</div>
                <div class="detail-row"><strong>Correctness risk</strong>${formatNumber.format(selected.correctness_risk || 0)}</div>
                <div class="detail-row"><strong>Change risk</strong>${formatNumber.format(selected.change_risk || 0)}</div>
                <div class="detail-row"><strong>Performance risk</strong>${formatNumber.format(selected.performance_risk || 0)}</div>
                <div class="detail-row"><strong>Architectural risk</strong>${formatNumber.format(selected.architectural_risk || 0)}</div>
                <div class="detail-row"><strong>Lines of code</strong>${formatNumber.format(selected.sloc || 0)}</div>
                <div class="detail-row"><strong>Maintainability signals</strong>${renderPills(categorySignals.maintainability || [])}</div>
                <div class="detail-row"><strong>Change signals</strong>${renderPills(categorySignals.change || [])}</div>
                <div class="detail-row"><strong>Performance signals</strong>${renderPills(categorySignals.performance || [])}</div>
                <div class="detail-row"><strong>Correctness signals</strong>${renderPills(categorySignals.correctness || [])}</div>
                <div class="detail-row"><strong>Architectural signals</strong>${renderPills(categorySignals.architectural || [])}</div>
                <div class="detail-row"><strong>Public API</strong>${formatNumber.format(evidence.public_api_count || 0)}</div>
                <div class="detail-row"><strong>Commits / churn</strong>${formatNumber.format(evidence.commit_count || 0)} / ${formatNumber.format(evidence.churn || 0)}</div>
                <div class="detail-row"><strong>Contributors / defects</strong>${formatNumber.format(evidence.contributor_count || 0)} / ${formatNumber.format(evidence.defect_commits || 0)}</div>
                <div class="detail-row"><strong>Tests</strong>${evidence.has_tests ? "evidence found" : "no direct evidence"}${evidence.test_count != null ? ` (${formatNumber.format(evidence.test_count)})` : ""}</div>
                <div class="detail-row"><strong>Failed / unknown tests</strong>${formatNumber.format(evidence.failed_tests || 0)} / ${formatNumber.format(evidence.unknown_tests || 0)}</div>
                <div class="detail-row"><strong>Layer violations</strong>${formatNumber.format(evidence.layer_violations || 0)}</div>
                <div class="detail-row"><strong>Cycle member</strong>${evidence.cycle_member ? "yes" : "no"}</div>
                <div class="detail-row"><strong>Outbound dependencies</strong>${renderPills(outbound)}</div>
                <div class="detail-row"><strong>Inbound dependencies</strong>${renderPills(inbound)}</div>
                <div class="detail-row"><strong>Benchmarks</strong>${perf.length ? perf.map(renderBenchmark).join("") : '<span class="muted">-</span>'}</div>
            </div>`;
    }

    function renderBenchmark(item) {
        const dispersionLabel = item.dispersion_label || "median_abs_dev";
        const dispersion = item.dispersion_ms == null ? "-" : `${formatNumber.format(item.dispersion_ms)} ms ${dispersionLabel}`;
        return `<div class="pill">${escapeHtml(item.name)}: ${formatNumber.format(item.mean_ms)} ms mean, ${dispersion}</div>`;
    }

    function renderFlamegraphs() {
        const container = byId("flamegraph-list");
        if (!container) return;

        if (!state.flamegraphs || !state.flamegraphs.length) {
            container.innerHTML = '<p class="muted">No flamegraphs loaded.</p>';
            byId("flamegraph-content").innerHTML = '<p class="muted">Generate flamegraphs using <code>open-overview.ps1 -Flamegraph</code> in an Administrator terminal.</p>';
            return;
        }

        container.innerHTML = state.flamegraphs.map(item => {
            const isActive = state.selectedFlamegraph === item.id;
            const isMissing = !item.available;
            return `<div class="flamegraph-item ${isActive ? 'is-active' : ''} ${isMissing ? 'is-error' : ''}" data-id="${escapeHtml(item.id)}">
                <h3>${escapeHtml(item.name)}</h3>
                <p>${escapeHtml(isMissing ? (item.issue || "Not generated") : item.id)}</p>
            </div>`;
        }).join("");

        container.querySelectorAll(".flamegraph-item").forEach(el => {
            el.addEventListener("click", () => {
                const id = el.dataset.id;
                state.selectedFlamegraph = id;
                renderFlamegraphs();
                loadSelectedFlamegraph();
            });
        });

        if (state.selectedFlamegraph === null && state.flamegraphs.length > 0) {
            state.selectedFlamegraph = state.flamegraphs[0].id;
            renderFlamegraphs();
            loadSelectedFlamegraph();
        }
    }

    async function loadSelectedFlamegraph() {
        const content = byId("flamegraph-content");
        const selected = state.flamegraphs.find(f => f.id === state.selectedFlamegraph);

        if (!selected) return;

        if (!selected.available) {
            content.innerHTML = `<div class="flamegraph-error">
                <h3>${escapeHtml(selected.name)}</h3>
                <p>${escapeHtml(selected.issue || selected.description || "No SVG is currently available for this profile.")}</p>
                <p>${escapeHtml((selected.workload_families || []).join(", ") || "-")}</p>
                <p>${escapeHtml((selected.benchmark_keys || []).join(", ") || "-")}</p>
            </div>`;
            return;
        }

        content.innerHTML = '<p class="muted">Loading SVG...</p>';
        try {
            // Path in JSON is relative to repo root, but we serve from repo root.
            // Viewer is at /viewer/, so path should be /target/analysis/flamegraphs/x.svg
            // Or relative: ../target/analysis/flamegraphs/x.svg
            const svgPath = `../target/analysis/${selected.path}?v=${viewerVersion}`;
            const response = await fetch(svgPath);
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            const svgText = await response.text();

            // To make the SVG interactive and fit properly, we might need to strip 
            // explicit width/height or wrap it.
            content.innerHTML = svgText;
        } catch (e) {
            content.innerHTML = `<div class="flamegraph-error">
                <h3>Failed to load SVG</h3>
                <p>${escapeHtml(e.message)}</p>
                <p>Ensure the file exists at <code>target/analysis/${escapeHtml(selected.path)}</code></p>
            </div>`;
        }
    }

    async function loadRunLog(runId, targetId = "run-log-output") {
        if (!runId) return;
        state.selectedRun = runId;
        renderSelectedRunProgress();
        const output = byId(targetId);
        if (!output) return;
        if (targetId === "overview-run-log") output.hidden = false;
        output.textContent = "Loading run log...";
        try {
            const response = await fetch(`/api/run/${encodeURIComponent(runId)}/log`, { cache: "no-store" });
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            output.textContent = await response.text();
        } catch (error) {
            output.textContent = `No log available from the dashboard server.\n${error.message}`;
        }
    }

    async function refreshRuns() {
        try {
            const previousFinished = state.lastObservedFinishedRun;
            state.runs = await loadJson(`/api/runs?v=${Date.now()}`);
            const latestFinished = [...state.runs].reverse().find((item) => item.finished_at);
            if (latestFinished && latestFinished.id !== previousFinished) {
                state.lastObservedFinishedRun = latestFinished.id;
                await loadDefaults();
                return;
            }
            renderOverview();
            renderRunLog();
        } catch {
            renderRunLog();
        }
    }

    async function triggerRun(endpoint, button) {
        const original = button.textContent;
        button.disabled = true;
        button.textContent = "Queued...";
        try {
            const response = await fetch(endpoint, { method: "POST" });
            if (!response.ok) {
                const payload = await response.json().catch(() => null);
                if (response.status === 409 && payload) {
                    byId("load-status").textContent = "Refresh already running.";
                    byId("load-detail").textContent = payload.active_run_id
                        ? `Waiting for ${payload.active_run_id} to finish.`
                        : "Wait for the current dashboard refresh to finish.";
                    await refreshRuns();
                    return;
                }
                throw new Error(`HTTP ${response.status}`);
            }
            const payload = await response.json();
            byId("load-status").textContent = `Queued ${payload.run_id}.`;
            byId("load-detail").textContent = "Refresh is running through the local dashboard server.";
            await refreshRuns();
        } catch (error) {
            byId("load-status").textContent = "Dashboard refresh unavailable.";
            byId("load-detail").textContent = `Start with scripts/open-overview.ps1 to enable refresh controls. ${error.message}`;
        } finally {
            button.disabled = false;
            button.textContent = original;
        }
    }

    async function loadJson(url) {
        const response = await fetch(url, { cache: "no-store" });
        if (!response.ok) {
            throw new Error(`${url} returned ${response.status}`);
        }
        return response.json();
    }

    async function loadDefaults() {
        const status = byId("load-status");
        const detail = byId("load-detail");
        const keys = ["catalog", "runs", "hotspots", "slowspots", "searchSpeed", "capacityReport", "resourceProfiles", "speedReport", "clones", "map", "flamegraphs", "correctness"];
        const fallbacks = {
            catalog: null,
            runs: [],
            hotspots: [],
            slowspots: [],
            searchSpeed: [],
            capacityReport: null,
            resourceProfiles: null,
            speedReport: null,
            clones: [],
            map: null,
            flamegraphs: [],
            correctness: null,
        };

        const settled = await Promise.allSettled(keys.map((key) => loadJson(sources[key])));
        const loaded = [];
        const missing = [];

        settled.forEach((result, index) => {
            const key = keys[index];
            if (result.status === "fulfilled") {
                state[key] = result.value;
                loaded.push(key);
            } else {
                state[key] = fallbacks[key];
                // flamegraphs is often missing if not generated, so we don't treat it as a loud error
                if (key !== "flamegraphs" && key !== "runs" && key !== "catalog") {
                    missing.push(`${key}: ${result.reason.message}`);
                }
            }
        });

        if (missing.length === 0) {
            status.textContent = "Loaded default JSON artifacts.";
            detail.textContent = "Data came from target/analysis.";
        } else if (loaded.length > 0) {
            status.textContent = `Loaded ${loaded.length} default artifact sets.`;
            detail.textContent = `Some default files were missing: ${missing.join("; ")}. Use Refresh to regenerate them.`;
        } else {
            status.textContent = "No artifacts loaded.";
            detail.textContent = `Default fetch failed: ${missing.join("; ")}. Start with scripts/open-overview.ps1 and use Refresh to regenerate artifacts.`;
        }
        renderAll();
    }

    function setupTabs() {
        document.querySelectorAll(".tab").forEach((button) => {
            button.addEventListener("click", () => {
                document.querySelectorAll(".tab").forEach((tab) => tab.classList.remove("is-active"));
                document.querySelectorAll(".tab-panel").forEach((panel) => panel.classList.remove("is-active"));
                button.classList.add("is-active");
                byId(button.dataset.tab).classList.add("is-active");

                if (button.dataset.tab === "performance-review") {
                    renderFlamegraphs();
                }
            });
        });
    }

    function renderAll() {
        renderOverview();
        renderHotspots();
        renderQualityDistribution();
        renderQualityFeed();
        renderSlowspots();
        renderSearchSpeed();
        renderSearchBudget();
        renderEditorBudget();
        renderSpeedReport();
        renderCapacityReport();
        renderResourceProfiles();
        renderClones();
        renderPerformanceScenarios();
        renderCorrectness();
        renderCorrectnessMatrix();
        renderMap();
        renderFlamegraphs();
        renderRunLog();
    }

    function renderQualityDistribution() {
        const target = byId("quality-distribution");
        if (!target) return;
        const hotspots = state.hotspots || [];
        if (!hotspots.length) {
            target.innerHTML = `<p class="muted">No hotspot data.</p>`;
            return;
        }
        let good = 0, warn = 0, bad = 0;
        const signalCounts = new Map();
        hotspots.forEach((h) => {
            const s = qualityScore(h);
            if (s >= 600) bad++;
            else if (s >= 300) warn++;
            else good++;
            pillValues(h.signals).forEach((sig) => signalCounts.set(sig, (signalCounts.get(sig) || 0) + 1));
        });
        const total = good + warn + bad;
        const sigSorted = [...signalCounts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6);
        const sigMax = sigSorted.length ? sigSorted[0][1] : 1;
        const goodPct = total ? (good / total) * 100 : 0;
        const warnPct = total ? (warn / total) * 100 : 0;
        const warnEnd = goodPct + warnPct;
        const buckets = [
            { cls: "good", label: "< 300", value: good },
            { cls: "warn", label: "300-599", value: warn },
            { cls: "bad", label: ">= 600", value: bad },
        ];
        const pie = `
            <div class="quality-pie-card">
                <div class="quality-pie" role="img" aria-label="Quality risk score buckets: ${good} below 300, ${warn} from 300 to 599, ${bad} at or above 600"
                    style="background: conic-gradient(var(--good) 0 ${goodPct}%, var(--warn) ${goodPct}% ${warnEnd}%, var(--bad) ${warnEnd}% 100%);">
                </div>
                <div class="quality-pie__summary">
                    <strong>${formatNumber.format(total)}</strong>
                    <span>items</span>
                </div>
                <div class="quality-pie__legend">
                    ${buckets.map((bucket) => `<div class="quality-pie__legend-item">
                        <span class="quality-pie__swatch quality-pie__swatch--${bucket.cls}"></span>
                        <span>${escapeHtml(bucket.label)}</span>
                        <strong>${formatNumber.format(bucket.value)}</strong>
                    </div>`).join("")}
                </div>
            </div>`;
        const signals = sigSorted.length
            ? `<div class="signal-bars">${sigSorted.map(([sig, count]) => `
                <div class="signal-bars__row">
                    <span>${escapeHtml(sig)}</span>
                    <div class="signal-bars__track"><span class="signal-bars__fill" style="width:${(count / sigMax) * 100}%"></span></div>
                    <span class="signal-bars__count">${count}</span>
                </div>`).join("")}</div>`
            : `<p class="muted">No signal data.</p>`;
        target.innerHTML = pie + signals;
    }

    function renderQualityFeed() {
        const target = byId("quality-feed");
        if (!target) return;
        const filter = (byId("quality-feed-filter")?.value || "").toLowerCase();
        const hotspots = (state.hotspots || []).map((h) => ({
            key: `hotspot:${h.name}:${h.start_line || ""}`,
            kind: "hotspot",
            name: h.name,
            score: qualityScore(h),
            signals: h.signals || [],
            details: `${formatNumber.format(h.sloc || 0)} SLOC`,
            raw: h,
            searchText: [h.kind, h.name, h.signals, h.sloc, qualityScore(h)].join(" "),
        }));
        const clones = (state.clones || []).map((c) => ({
            key: `clone:${c.hash || c.group_hash || c.name || ""}`,
            kind: "clone",
            name: `clone ${c.hash?.substring(0, 8) || ""} (${c.instance_count || c.instances?.length || 0}x)`,
            score: c.score || 0,
            signals: c.signals || [],
            details: `${c.token_count || 0} tokens`,
            raw: c,
            searchText: [c.engine, c.hash, c.score, c.token_count, c.signals, ...(c.instances || []).map((inst) => inst.file_path)].join(" "),
        }));
        const merged = [...hotspots, ...clones]
            .filter((it) => !filter || it.searchText.toLowerCase().includes(filter))
            .sort((a, b) => b.score - a.score)
            .slice(0, 60);
        if (!merged.length) {
            target.innerHTML = `<p class="muted">No items match.</p>`;
            return;
        }
        target.innerHTML = merged.map((it) => {
            const cls = riskClass(it.score, 300, 600);
            const tail = (it.name || "").split(/[\\/]/).pop();
            const isExpanded = state.expandedQualityKey === it.key;
            return `<button type="button" class="quality-feed__row ${isExpanded ? "is-expanded" : ""}" data-quality-key="${escapeHtml(it.key)}" aria-expanded="${isExpanded ? "true" : "false"}">
                <span class="pill">${escapeHtml(it.kind)}</span>
                <span class="quality-feed__name"><code>${escapeHtml(tail)}</code><span class="muted quality-feed__detail">${escapeHtml(it.details)}</span></span>
                <span class="${cls}">${formatNumber.format(it.score)}</span>
            </button>
            ${isExpanded ? renderQualityFeedDetail(it) : ""}`;
        }).join("");
        target.querySelectorAll("[data-quality-key]").forEach((button) => {
            button.addEventListener("click", () => {
                state.expandedQualityKey = state.expandedQualityKey === button.dataset.qualityKey ? null : button.dataset.qualityKey;
                renderQualityFeed();
            });
        });
    }

    function renderMetricGrid(metrics) {
        return `<div class="quality-detail__metrics">${metrics.map(({ label, value, cls }) => `
            <div class="quality-detail__metric">
                <span>${escapeHtml(label)}</span>
                <strong class="${cls || ""}">${value}</strong>
            </div>`).join("")}</div>`;
    }

    function cloneTouchesFile(clone, fileName) {
        const needle = normalizePath(fileName);
        return (clone.instances || []).some((inst) => normalizePath(inst.file_path) === needle);
    }

    function hotspotTouchesClone(hotspot, clone) {
        const files = new Set((clone.instances || []).map((inst) => normalizePath(inst.file_path)));
        return files.has(normalizePath(hotspot.name));
    }

    function renderQualityFeedDetail(item) {
        if (item.kind === "clone") {
            const clone = item.raw || {};
            const relatedHotspots = (state.hotspots || [])
                .filter((hotspot) => hotspotTouchesClone(hotspot, clone))
                .sort((a, b) => qualityScore(b) - qualityScore(a))
                .slice(0, 8);
            return `<div class="quality-detail">
                ${renderCloneDetail(clone)}
                <div class="quality-detail__section">
                    <h4>Related quality metrics</h4>
                    ${relatedHotspots.length ? relatedHotspots.map((hotspot) => renderHotspotDetail(hotspot, { compact: true })).join("") : `<p class="muted">No matching quality hotspot records.</p>`}
                </div>
            </div>`;
        }

        const hotspot = item.raw || {};
        const matchingClones = (state.clones || [])
            .filter((clone) => cloneTouchesFile(clone, hotspot.name))
            .sort((a, b) => Number(b.score || 0) - Number(a.score || 0))
            .slice(0, 8);
        return `<div class="quality-detail">
            ${renderHotspotDetail(hotspot)}
            <div class="quality-detail__section">
                <h4>Clone metrics for this file</h4>
                ${matchingClones.length ? matchingClones.map((clone) => renderCloneDetail(clone, { compact: true })).join("") : `<p class="muted">No clone groups found for this file.</p>`}
            </div>
        </div>`;
    }

    function renderHotspotDetail(hotspot, options = {}) {
        const score = qualityScore(hotspot);
        const metrics = [
            { label: "Quality", value: formatNumber.format(score), cls: riskClass(score, 300, 600) },
            { label: "Cog", value: formatNumber.format(hotspot.cognitive || 0) },
            { label: "Cyc", value: formatNumber.format(hotspot.cyclomatic || 0) },
            { label: "MI", value: formatNumber.format(hotspot.mi || 0) },
            { label: "Halstead Effort", value: formatNumber.format(hotspot.effort || 0) },
            { label: "SLOC", value: formatNumber.format(hotspot.sloc || 0) },
            { label: "Start Line", value: escapeHtml(hotspot.start_line || "-") },
        ];
        return `<div class="quality-detail__section ${options.compact ? "quality-detail__section--compact" : ""}">
            <h4>${escapeHtml(hotspot.name || "Quality metrics")}</h4>
            ${renderMetricGrid(metrics)}
            <div class="quality-detail__signals">${renderPills(hotspot.signals)}</div>
        </div>`;
    }

    function renderCloneDetail(clone, options = {}) {
        const hash = clone.hash || clone.group_hash || "";
        const instances = clone.instances || [];
        const fileCount = clone.file_count ?? new Set(instances.map((inst) => normalizePath(inst.file_path))).size;
        const score = Number(clone.score || 0);
        const metrics = [
            { label: "Engine", value: escapeHtml(clone.engine || "token") },
            { label: "Group Hash", value: `<code>${escapeHtml(hash.substring(0, options.compact ? 8 : 16) || "-")}</code>` },
            { label: "Instances", value: formatNumber.format(clone.instance_count || instances.length || 0) },
            { label: "Files", value: formatNumber.format(fileCount) },
            { label: "Score", value: formatNumber.format(score), cls: riskClass(score, 20, 40) },
            { label: "Token Count", value: formatNumber.format(clone.token_count || 0) },
            { label: "Max Line Span", value: formatNumber.format(clone.max_line_span || 0) },
        ];
        const locations = instances.length
            ? `<div class="quality-detail__locations">${instances.slice(0, options.compact ? 4 : 12).map((inst) => `
                <code>${escapeHtml(inst.file_path)}:${escapeHtml(inst.start_line)}-${escapeHtml(inst.end_line)}</code>`).join("")}</div>`
            : `<p class="muted">No clone locations recorded.</p>`;
        return `<div class="quality-detail__section ${options.compact ? "quality-detail__section--compact" : ""}">
            <h4>${escapeHtml(options.compact ? `Clone ${hash.substring(0, 8)}` : "Clone metrics")}</h4>
            ${renderMetricGrid(metrics)}
            <div class="quality-detail__signals">${renderPills(clone.signals)}</div>
            ${locations}
        </div>`;
    }

    function titleCaseMetricName(value) {
        return String(value || "")
            .replace(/^search_/, "")
            .replaceAll("_", " ")
            .replace(/\b\w/g, (letter) => letter.toUpperCase());
    }

    function budgetParameterLabel(item) {
        if (item.parameter_label) return item.parameter_label;
        const rawValue = String(item.name || "").split("/").pop();
        const numericValue = Number(rawValue);
        const key = item.benchmark_key || item.name || "";
        if (!rawValue || rawValue === String(item.name || "")) return "";
        if (Number.isFinite(numericValue)) {
            if (key.includes("file_size") || key.includes("bytes")) return formatBytes(numericValue);
            if (key.includes("aggregate_size")) return `${formatNumber.format(numericValue)} tabs`;
            if (key.includes("tab_count")) return `${formatNumber.format(numericValue)} tabs`;
            if (key.includes("split")) return `${formatNumber.format(numericValue)} splits`;
        }
        return rawValue;
    }

    function budgetLabelParts(item) {
        const key = item.benchmark_key || String(item.name || "").split("/")[0];
        const title = item.scenario_label || titleCaseMetricName(key);
        const details = [
            budgetParameterLabel(item),
            item.workload_family || item.mode,
            item.latency_kind,
        ].filter(Boolean);
        return { title, subtitle: [...new Set(details)].join(" - ") };
    }

    function renderBudgetBars(targetId, items) {
        const target = byId(targetId);
        if (!target) return;
        if (!items.length) {
            target.innerHTML = `<div class="budget-bars__empty">No data. Run the relevant refresh.</div>`;
            return;
        }
        const ratios = items
            .map((it) => {
                const meanMs = (it.mean_ns || 0) / 1_000_000;
                const budget = it.threshold_ms || 0;
                if (!budget) return null;
                return {
                    label: it.scenario_label || it.name,
                    labelParts: budgetLabelParts(it),
                    meanMs,
                    budget,
                    ratio: meanMs / budget,
                };
            })
            .filter(Boolean)
            .sort((a, b) => b.ratio - a.ratio)
            .slice(0, 30);
        if (!ratios.length) {
            target.innerHTML = `<div class="budget-bars__empty">No budget thresholds available.</div>`;
            return;
        }
        const maxRatio = Math.max(1.2, ...ratios.map((r) => r.ratio));
        target.innerHTML = ratios.map((r) => {
            const fillPct = (r.ratio / maxRatio) * 100;
            const thresholdPct = (1 / maxRatio) * 100;
            const cls = r.ratio > 1 ? "bad" : r.ratio > 0.85 ? "warn" : "good";
            const fillClass = cls === "good" ? "" : `budget-bars__fill--${cls}`;
            return `<div class="budget-bars__row" title="${escapeHtml(r.label)}">
                <span class="budget-bars__label">
                    <strong>${escapeHtml(r.labelParts.title)}</strong>
                    ${r.labelParts.subtitle ? `<span>${escapeHtml(r.labelParts.subtitle)}</span>` : ""}
                </span>
                <div class="budget-bars__track">
                    <div class="budget-bars__fill ${fillClass}" style="width:${Math.min(100, fillPct)}%"></div>
                    <div class="budget-bars__threshold" style="left:${thresholdPct}%"></div>
                </div>
                <span class="budget-bars__value">${formatNumber.format(r.ratio * 100)}%</span>
            </div>`;
        }).join("");
    }

    function renderSearchBudget() {
        renderBudgetBars("search-budget", state.searchSpeed || []);
    }

    function renderEditorBudget() {
        renderBudgetBars("editor-budget", state.slowspots || []);
    }

    function renderCorrectnessMatrix() {
        const target = byId("correctness-matrix");
        if (!target) return;
        const layers = state.correctness?.layers || [];
        if (!layers.length) {
            target.innerHTML = `<p class="muted">No correctness data.</p>`;
            return;
        }
        target.innerHTML = layers.map((layer) => {
            const total = layer.total || 0;
            const passed = layer.passed || 0;
            const failed = layer.failed || 0;
            const unknown = layer.unknown || 0;
            const ratio = layer.failed_ratio != null
                ? Number(layer.failed_ratio)
                : total ? failed / total : 0;
            let cls = "ok";
            if (failed > 0) cls = "bad";
            else if (unknown > 0 && total) cls = "warn";
            else if (!total) cls = "stale";
            const pct = (n) => total ? `${(n / total) * 100}%` : "0%";
            const isActive = state.selectedLayer === layer.name;
            return `<div class="layer-matrix__cell layer-matrix__cell--${cls} ${isActive ? "is-active" : ""}" data-layer="${escapeHtml(layer.name)}">
                <div class="layer-matrix__name">${escapeHtml(layer.name)}</div>
                <div class="layer-matrix__counts">
                    <span class="risk-good">${passed} pass</span>
                    <span class="${failed ? "risk-bad" : "muted"}">${failed} fail</span>
                    <span class="muted">${unknown} unknown</span>
                </div>
                <div class="layer-matrix__bar">
                    <span class="passed" style="width:${pct(passed)}"></span>
                    <span class="failed" style="width:${pct(failed)}"></span>
                    <span class="unknown" style="width:${pct(unknown)}"></span>
                </div>
                <div class="muted" style="font-size:11px">${total} tests · ${formatNumber.format(ratio * 100)}% failing</div>
            </div>`;
        }).join("");
        target.querySelectorAll(".layer-matrix__cell").forEach((cell) => {
            cell.addEventListener("click", () => {
                const layer = cell.dataset.layer;
                state.selectedLayer = state.selectedLayer === layer ? null : layer;
                renderCorrectnessMatrix();
                renderCorrectness();
            });
        });
    }

    byId("viewer-version").textContent = viewerVersion;
    setupTabs();
    byId("hotspots-filter")?.addEventListener("input", renderHotspots);
    byId("slowspots-filter")?.addEventListener("input", renderSlowspots);
    byId("search-speed-filter")?.addEventListener("input", renderSearchSpeed);
    byId("resource-profiles-filter")?.addEventListener("input", renderResourceProfiles);
    byId("clones-filter")?.addEventListener("input", renderClones);
    byId("correctness-filter")?.addEventListener("input", renderCorrectness);
    byId("correctness-show-all")?.addEventListener("change", renderCorrectness);
    byId("quality-feed-filter")?.addEventListener("input", renderQualityFeed);
    byId("map-filter").addEventListener("input", renderMap);
    byId("map-layout").addEventListener("change", (event) => {
        state.mapLayout = event.target.value;
        renderMap();
    });
    byId("map-metric").addEventListener("change", (event) => {
        state.mapMetric = event.target.value;
        renderMap();
    });
    byId("map-focus").addEventListener("change", (event) => {
        state.focusMode = event.target.checked;
        renderMap();
    });
    byId("map-zoom").addEventListener("input", (event) => {
        state.mapZoom = Number(event.target.value);
        byId("map-zoom-value").textContent = `${Math.round(state.mapZoom * 100)}%`;
        renderMap();
    });
    document.querySelectorAll("[data-overview-risk-mode]").forEach((button) => {
        button.addEventListener("click", () => {
            state.overviewRiskMode = button.dataset.overviewRiskMode;
            document.querySelectorAll("[data-overview-risk-mode]").forEach((item) => {
                const active = item === button;
                item.classList.toggle("is-active", active);
                item.setAttribute("aria-pressed", active ? "true" : "false");
            });
            renderRiskTreemap();
        });
    });
    byId("overview-risk-filter")?.addEventListener("change", (event) => {
        state.overviewRiskFilter = event.target.value;
        renderRiskTreemap();
    });
    document.querySelectorAll("[data-run]").forEach((button) => {
        button.addEventListener("click", () => triggerRun("/api/run/all", button));
    });
    document.querySelectorAll("[data-run-category]").forEach((button) => {
        button.addEventListener("click", () => triggerRun(`/api/run/category/${encodeURIComponent(button.dataset.runCategory)}`, button));
    });
    document.querySelectorAll("[data-run-item]").forEach((button) => {
        button.addEventListener("click", () => triggerRun(`/api/run/item/${encodeURIComponent(button.dataset.runItem)}`, button));
    });
    window.setInterval(refreshRuns, 5000);
    loadDefaults();
})();
