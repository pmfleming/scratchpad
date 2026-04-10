(function () {
    const DATA_PATH = "./architecture_map_data.json";

    const loader = document.getElementById("loader");
    const loaderError = document.getElementById("loader-error");
    const statusDetail = document.getElementById("status-detail");
    const status = document.getElementById("status");
    const infoPanel = document.getElementById("info");
    const fileInput = document.getElementById("file-input");

    let cy = null;
    let selectedNodeId = null;

    function setStatus(text, detail) {
        status.innerHTML = `<div><strong>Data:</strong> ${text}</div><div id="status-detail" style="margin-top: 8px; color: var(--muted);">${detail}</div>`;
    }

    function setLoaderError(message) {
        loaderError.textContent = message || "";
    }

    function hideLoader() {
        loader.classList.add("hidden");
    }

    function showInfoPanel() {
        infoPanel.classList.add("visible");
    }

    function hideInfoPanel() {
        infoPanel.classList.remove("visible");
    }

    function updateInfoPanel(node) {
        document.getElementById("info-title").innerText = node.data("id");
        document.getElementById("info-total").innerText = (node.data("total_score") || 0).toFixed(1);
        document.getElementById("info-comp").innerText = (node.data("comp_score") || 0).toFixed(1);
        document.getElementById("info-perf-score").innerText = (node.data("perf_score") || 0).toFixed(1);
        document.getElementById("info-sloc").innerText = node.data("sloc") || 0;
        document.getElementById("info-signals").innerText = node.data("signals") || "stable";
        document.getElementById("info-perf-kind").innerText = node.data("perf_kind") || "-";

        const perfPanel = document.getElementById("info-perf");
        if (node.data("is_slow")) {
            perfPanel.style.display = "block";
            document.getElementById("info-perf-list").innerHTML = node.data("perf_info");
        } else {
            perfPanel.style.display = "none";
        }

        const depsPanel = document.getElementById("info-deps");
        const revDepsPanel = document.getElementById("info-rev-deps");
        if (selectedNodeId === node.id()) {
            const outgoers = node.outgoers().nodes().sort((a, b) => a.id().localeCompare(b.id()));
            const incomers = node.incomers().nodes().sort((a, b) => a.id().localeCompare(b.id()));

            depsPanel.style.display = outgoers.length > 0 ? "block" : "none";
            revDepsPanel.style.display = incomers.length > 0 ? "block" : "none";
            document.getElementById("info-deps-list").innerHTML = outgoers
                .map((n) => `<span class="dep-item" data-node-id="${n.id()}">${n.id()}</span>`)
                .join("");
            document.getElementById("info-rev-deps-list").innerHTML = incomers
                .map((n) => `<span class="dep-item" data-node-id="${n.id()}">${n.id()}</span>`)
                .join("");
        } else {
            depsPanel.style.display = "none";
            revDepsPanel.style.display = "none";
        }

        showInfoPanel();
    }

    function selectNodeById(id) {
        if (!cy) {
            return;
        }

        const node = cy.getElementById(id);
        if (node.length > 0) {
            cy.animate({ center: { eles: node }, zoom: Math.max(cy.zoom(), 1.15) }, { duration: 250 });
            node.trigger("tap");
        }
    }

    function attachDependencyClickHandlers() {
        document.querySelectorAll(".dep-item").forEach((element) => {
            element.addEventListener("click", () => {
                const id = element.getAttribute("data-node-id");
                if (id) {
                    selectNodeById(id);
                }
            });
        });
    }

    function buildCy(elements) {
        cy = cytoscape({
            container: document.getElementById("cy"),
            elements: [...elements.nodes, ...elements.edges],
            style: [
                {
                    selector: "node",
                    style: {
                        label: "data(label)",
                        "text-valign": "center",
                        "text-halign": "center",
                        "text-wrap": "wrap",
                        "text-max-width": "150px",
                        color: "#f7f7f7",
                        "font-size": "12px",
                        "font-weight": 700,
                        "text-outline-width": 2,
                        "text-outline-color": "#16181d",
                        "background-color": "#333",
                        "border-width": 2,
                        "border-color": "#f3f3f3",
                        "shadow-blur": 18,
                        "shadow-color": "#000",
                        "shadow-opacity": 0.35,
                        "shadow-offset-x": 0,
                        "shadow-offset-y": 4,
                    },
                },
                {
                    selector: "node[is_group]",
                    style: {
                        "text-valign": "top",
                        "text-halign": "center",
                        color: "data(bg_color)",
                        "font-weight": "bold",
                        "font-size": "18px",
                        "background-color": "data(bg_color)",
                        "background-opacity": "data(bg_opacity)",
                        "border-width": 3,
                        "border-color": "data(bg_color)",
                        shape: "round-rectangle",
                        padding: 28,
                    },
                },
                {
                    selector: "node[!is_group]",
                    style: {
                        shape: "round-rectangle",
                        width: (ele) => Math.max(135, 90 + Math.sqrt(ele.data("total_score") || 1) * 8),
                        height: (ele) => Math.max(72, 48 + Math.sqrt(ele.data("total_score") || 1) * 4),
                        "background-fill": "linear-gradient",
                        "background-gradient-direction": "to-right",
                        "background-gradient-stop-colors": "data(comp_risk_color) data(comp_risk_color) data(perf_risk_color) data(perf_risk_color)",
                        "background-gradient-stop-positions": "0 50 50 100",
                        "border-width": 3,
                        "border-color": "#f3f3f3",
                    },
                },
                {
                    selector: "edge",
                    style: {
                        width: 3,
                        "line-color": "#8d8d8d",
                        "target-arrow-color": "#8d8d8d",
                        "target-arrow-shape": "triangle",
                        "curve-style": "taxi",
                        "arrow-scale": 1.2,
                        opacity: 0.85,
                    },
                },
                {
                    selector: ".depends-on-green",
                    style: {
                        "line-color": "#00ff88",
                        "target-arrow-color": "#00ff88",
                        width: 7,
                        opacity: 1,
                        "arrow-scale": 1.4,
                        "z-index": 10,
                    },
                },
                {
                    selector: ".depended-by-red",
                    style: {
                        "line-color": "#ff5c5c",
                        "target-arrow-color": "#ff5c5c",
                        width: 7,
                        opacity: 1,
                        "arrow-scale": 1.4,
                        "z-index": 10,
                    },
                },
                {
                    selector: ".highlight-green",
                    style: {
                        "border-width": 8,
                        "border-color": "#00ff88",
                        "font-size": "13px",
                        "shadow-blur": 28,
                        "shadow-color": "#00ff88",
                        "shadow-opacity": 0.45,
                        "z-index": 20,
                    },
                },
                {
                    selector: ".highlight-red",
                    style: {
                        "border-width": 8,
                        "border-color": "#ff5c5c",
                        "font-size": "13px",
                        "shadow-blur": 28,
                        "shadow-color": "#ff5c5c",
                        "shadow-opacity": 0.45,
                        "z-index": 20,
                    },
                },
                {
                    selector: ".highlight-center",
                    style: {
                        "border-width": 12,
                        "border-color": "#9cdcfe",
                        "font-size": "14px",
                        "shadow-blur": 34,
                        "shadow-color": "#9cdcfe",
                        "shadow-opacity": 0.7,
                        "z-index": 30,
                    },
                },
                {
                    selector: ".highlight-neighborhood",
                    style: {
                        "font-size": "13px",
                        "z-index": 15,
                    },
                },
            ],
            layout: {
                name: "dagre",
                padding: 100,
                spacingFactor: 1.4,
                nodeSep: 90,
                rankSep: 180,
            },
        });

        cy.ready(() => {
            const moduleNodes = cy.nodes("[!is_group]");
            if (moduleNodes.length > 0) {
                cy.fit(moduleNodes, 80);
            }
        });

        cy.on("mouseover", "node[!is_group]", (evt) => {
            updateInfoPanel(evt.target);
        });

        cy.on("mouseout", "node", () => {
            if (!selectedNodeId) {
                hideInfoPanel();
                return;
            }
            const selected = cy.getElementById(selectedNodeId);
            if (selected.length > 0) {
                updateInfoPanel(selected);
                attachDependencyClickHandlers();
            }
        });

        cy.on("tap", "node[!is_group]", (evt) => {
            const node = evt.target;
            selectedNodeId = node.id();

            cy.elements().removeClass("highlight-green highlight-red highlight-center highlight-neighborhood depends-on-green depended-by-red");
            node.addClass("highlight-center");

            const outgoers = node.outgoers();
            outgoers.nodes().addClass("highlight-neighborhood");
            outgoers.edges().addClass("depends-on-green");
            outgoers.nodes().addClass("highlight-green");

            const incomers = node.incomers();
            incomers.nodes().addClass("highlight-neighborhood");
            incomers.edges().addClass("depended-by-red");
            incomers.nodes().addClass("highlight-red");

            updateInfoPanel(node);
            attachDependencyClickHandlers();
        });

        cy.on("tap", (evt) => {
            if (evt.target === cy) {
                selectedNodeId = null;
                cy.elements().removeClass("highlight-green highlight-red highlight-center highlight-neighborhood depends-on-green depended-by-red");
                hideInfoPanel();
            }
        });
    }

    async function fetchDefaultData() {
        const response = await fetch(DATA_PATH, { cache: "no-store" });
        if (!response.ok) {
            throw new Error(`Failed to load ${DATA_PATH} (${response.status})`);
        }
        return response.json();
    }

    async function readSelectedFile(file) {
        const text = await file.text();
        return JSON.parse(text);
    }

    function renderPayload(payload, sourceLabel) {
        if (!payload || !payload.graph || !payload.graph.nodes || !payload.graph.edges) {
            throw new Error("Invalid architecture map payload.");
        }

        buildCy(payload.graph);
        setStatus(
            sourceLabel,
            `${payload.meta?.node_count ?? payload.graph.nodes.length} nodes, ${payload.meta?.edge_count ?? payload.graph.edges.length} edges`
        );
        hideLoader();
    }

    fileInput.addEventListener("change", async (event) => {
        const file = event.target.files && event.target.files[0];
        if (!file) {
            return;
        }

        try {
            const payload = await readSelectedFile(file);
            renderPayload(payload, `loaded from ${file.name}`);
            setLoaderError("");
        } catch (error) {
            setLoaderError(error.message || String(error));
        }
    });

    fetchDefaultData()
        .then((payload) => {
            renderPayload(payload, `loaded ${DATA_PATH}`);
            statusDetail.textContent = "Default JSON loaded successfully.";
        })
        .catch((error) => {
            setStatus("manual load required", "Automatic fetch failed. Pick the JSON file below.");
            setLoaderError(error.message || String(error));
        });
})();