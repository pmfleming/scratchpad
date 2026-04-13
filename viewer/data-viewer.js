(function () {
    const viewerVersion = window.SCRATCHPAD_VIEWER_VERSION || "dev";
    const sources = {
        hotspots: `../target/analysis/hotspots.json?v=${viewerVersion}`,
        slowspots: `../target/analysis/slowspots.json?v=${viewerVersion}`,
        clones: `../target/analysis/clones.json?v=${viewerVersion}`,
        map: `../target/analysis/map.json?v=${viewerVersion}`,
    };

    const state = {
        hotspots: [],
        slowspots: [],
        clones: [],
        map: null,
        selectedModule: null,
        mapZoom: 0.65,
        mapLayout: 'folder',
        mapMetric: 'total_score',
        focusMode: false,
    };

    const formatNumber = new Intl.NumberFormat(undefined, {
        maximumFractionDigits: 2,
    });

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

        renderSummary("hotspots-summary", [
            metricCard("Records", state.hotspots.length),
            metricCard("Files", files.size),
            metricCard("Worst score", worst ? formatNumber.format(worst.score) : "-"),
            metricCard("Worst item", worst ? worst.name.split(/[\\/]/).pop() : "-"),
        ]);

        renderTable(
            "hotspots-table",
            ["Rank", "Kind", "Name", "Score", "Cog", "Cyc", "MI", "SLOC", "Signals"],
            filtered.map((item, index) => {
                const scoreClass = riskClass(item.score, 300, 600);
                return `<tr>
                    <td>${index + 1}</td>
                    <td><span class="pill">${escapeHtml(item.kind)}</span></td>
                    <td><code>${escapeHtml(item.name)}</code><div class="muted">line ${escapeHtml(item.start_line)}</div></td>
                    <td class="${scoreClass}">${formatNumber.format(item.score)}</td>
                    <td>${formatNumber.format(item.cognitive)}</td>
                    <td>${formatNumber.format(item.cyclomatic)}</td>
                    <td>${formatNumber.format(item.mi)}</td>
                    <td>${formatNumber.format(item.sloc)}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
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
            ["Benchmark", "Kind", "Mean", "Median", "P95", "Threshold", "Targets", "Signals"],
            filtered.map((item) => {
                const meanMs = item.mean_ns / 1_000_000;
                const medianMs = item.median_ns / 1_000_000;
                const p95Ms = item.p95_ns == null ? null : item.p95_ns / 1_000_000;
                const scoreClass = meanMs > item.threshold_ms ? "risk-bad" : "risk-good";
                return `<tr>
                    <td><code>${escapeHtml(item.name)}</code></td>
                    <td><span class="pill">${escapeHtml(item.benchmark_kind)}</span></td>
                    <td class="${scoreClass}">${formatNumber.format(meanMs)} ms</td>
                    <td>${formatNumber.format(medianMs)} ms</td>
                    <td>${p95Ms == null ? "-" : `${formatNumber.format(p95Ms)} ms`}</td>
                    <td>${formatNumber.format(item.threshold_ms)} ms</td>
                    <td>${renderPills(item.targets || [])}</td>
                    <td>${renderPills(item.signals)}</td>
                </tr>`;
            })
        );
    }

    function renderPills(value) {
        const values = Array.isArray(value)
            ? value
            : String(value || "")
                .split(",")
                .map((item) => item.trim())
                .filter(Boolean);
        if (!values.length) {
            return '<span class="muted">-</span>';
        }
        return values.map((item) => `<span class="pill">${escapeHtml(item)}</span>`).join("");
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
                                            state.mapMetric === 'architectural' ? right.architectural_risk :
                                            state.mapMetric === 'churn' ? right.churn : right.total_score;
                        const metricLeft = state.mapMetric === 'maintainability' ? left.maintainability_risk :
                                           state.mapMetric === 'change' ? left.change_risk :
                                           state.mapMetric === 'performance' ? left.performance_risk :
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
                                            state.mapMetric === 'architectural' ? right.architectural_risk :
                                            state.mapMetric === 'churn' ? right.churn : right.total_score;
                        const metricLeft = state.mapMetric === 'maintainability' ? left.maintainability_risk :
                                           state.mapMetric === 'change' ? left.change_risk :
                                           state.mapMetric === 'performance' ? left.performance_risk :
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
        
        const metricValue = state.mapMetric === 'maintainability' ? node.maintainability_risk :
                            state.mapMetric === 'change' ? node.change_risk :
                            state.mapMetric === 'performance' ? node.performance_risk :
                            state.mapMetric === 'architectural' ? node.architectural_risk :
                            state.mapMetric === 'churn' ? node.churn : node.total_score;
        const fill = scoreFill(metricValue || 0, state.mapMetric);
        const label = shortenLabel(node.id);
        const score = formatNumber.format(metricValue || 0);
        const chips = [
            `M ${Math.round(node.maintainability_risk || 0)}`,
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

    function scoreFill(score, metric) {
        let bad = 600;
        let warn = 300;
        if (metric === 'maintainability' || metric === 'architectural') { bad = 350; warn = 150; }
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
            const getMetric = (node) => {
                return state.mapMetric === 'maintainability' ? node.maintainability_risk :
                       state.mapMetric === 'change' ? node.change_risk :
                       state.mapMetric === 'performance' ? node.performance_risk :
                       state.mapMetric === 'architectural' ? node.architectural_risk :
                       state.mapMetric === 'churn' ? node.churn : node.total_score;
            };
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
                <div class="detail-row"><strong>Maintainability risk</strong>${formatNumber.format(selected.maintainability_risk || 0)}</div>
                <div class="detail-row"><strong>Change risk</strong>${formatNumber.format(selected.change_risk || 0)}</div>
                <div class="detail-row"><strong>Performance risk</strong>${formatNumber.format(selected.performance_risk || 0)}</div>
                <div class="detail-row"><strong>Architectural risk</strong>${formatNumber.format(selected.architectural_risk || 0)}</div>
                <div class="detail-row"><strong>Lines of code</strong>${formatNumber.format(selected.sloc || 0)}</div>
                <div class="detail-row"><strong>Maintainability signals</strong>${renderPills(categorySignals.maintainability || [])}</div>
                <div class="detail-row"><strong>Change signals</strong>${renderPills(categorySignals.change || [])}</div>
                <div class="detail-row"><strong>Performance signals</strong>${renderPills(categorySignals.performance || [])}</div>
                <div class="detail-row"><strong>Architectural signals</strong>${renderPills(categorySignals.architectural || [])}</div>
                <div class="detail-row"><strong>Public API</strong>${formatNumber.format(evidence.public_api_count || 0)}</div>
                <div class="detail-row"><strong>Commits / churn</strong>${formatNumber.format(evidence.commit_count || 0)} / ${formatNumber.format(evidence.churn || 0)}</div>
                <div class="detail-row"><strong>Contributors / defects</strong>${formatNumber.format(evidence.contributor_count || 0)} / ${formatNumber.format(evidence.defect_commits || 0)}</div>
                <div class="detail-row"><strong>Tests</strong>${evidence.has_tests ? "evidence found" : "no direct evidence"}</div>
                <div class="detail-row"><strong>Layer violations</strong>${formatNumber.format(evidence.layer_violations || 0)}</div>
                <div class="detail-row"><strong>Cycle member</strong>${evidence.cycle_member ? "yes" : "no"}</div>
                <div class="detail-row"><strong>Outbound dependencies</strong>${renderPills(outbound)}</div>
                <div class="detail-row"><strong>Inbound dependencies</strong>${renderPills(inbound)}</div>
                <div class="detail-row"><strong>Benchmarks</strong>${perf.length ? perf.map(renderBenchmark).join("") : '<span class="muted">-</span>'}</div>
            </div>`;
    }

    function renderBenchmark(item) {
        const p95 = item.p95_ms == null ? "-" : `${formatNumber.format(item.p95_ms)} ms p95`;
        return `<div class="pill">${escapeHtml(item.name)}: ${formatNumber.format(item.mean_ms)} ms mean, ${p95}</div>`;
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
        try {
            const [hotspots, slowspots, clones, map] = await Promise.all([
                loadJson(sources.hotspots),
                loadJson(sources.slowspots),
                loadJson(sources.clones),
                loadJson(sources.map),
            ]);
            state.hotspots = hotspots;
            state.slowspots = slowspots;
            state.clones = clones;
            state.map = map;
            status.textContent = "Loaded default JSON artifacts.";
            detail.textContent = "Data came from target/analysis.";
        } catch (error) {
            status.textContent = "Manual JSON load available.";
            detail.textContent = `Default fetch failed: ${error.message}. Pick JSON files above, or serve the repo root with a local HTTP server.`;
        }
        renderAll();
    }

    function readJsonFile(inputId, stateKey, renderFn) {
        byId(inputId).addEventListener("change", async (event) => {
            const file = event.target.files?.[0];
            if (!file) {
                return;
            }
            const text = await file.text();
            state[stateKey] = JSON.parse(text);
            byId("load-status").textContent = `Loaded ${file.name}.`;
            byId("load-detail").textContent = "Manual file input updated the viewer state.";
            renderFn();
        });
    }

    function setupTabs() {
        document.querySelectorAll(".tab").forEach((button) => {
            button.addEventListener("click", () => {
                document.querySelectorAll(".tab").forEach((tab) => tab.classList.remove("is-active"));
                document.querySelectorAll(".tab-panel").forEach((panel) => panel.classList.remove("is-active"));
                button.classList.add("is-active");
                byId(button.dataset.tab).classList.add("is-active");
            });
        });
    }

    function renderAll() {
        renderHotspots();
        renderSlowspots();
        renderClones();
        renderMap();
    }

    byId("viewer-version").textContent = viewerVersion;
    setupTabs();
    byId("hotspots-filter").addEventListener("input", renderHotspots);
    byId("slowspots-filter").addEventListener("input", renderSlowspots);
    byId("clones-filter").addEventListener("input", renderClones);
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
    readJsonFile("hotspots-file", "hotspots", renderHotspots);
    readJsonFile("slowspots-file", "slowspots", renderSlowspots);
    readJsonFile("clones-file", "clones", renderClones);
    readJsonFile("map-file", "map", renderMap);
    loadDefaults();
})();
