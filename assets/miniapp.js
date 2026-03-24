import sdk from "https://esm.sh/@farcaster/frame-sdk";
sdk.actions.ready();

const BASE = "%%BASE_URL%%";

window.switchTab = function(tab) {
    document.querySelectorAll(".tab").forEach(t => t.classList.remove("active"));
    document.querySelectorAll(".panel").forEach(p => p.classList.remove("active"));
    document.querySelector(`[data-tab="${tab}"]`).classList.add("active");
    document.getElementById(tab).classList.add("active");
};

window.searchAgents = async function() {
    const q = document.getElementById("search-input").value;
    await loadAgents(q);
};

async function loadAgents(query) {
    const list = document.getElementById("agents-list");
    try {
        const url = query ? `${BASE}/api/registry?q=${encodeURIComponent(query)}&limit=20` : `${BASE}/api/registry?limit=20`;
        const resp = await fetch(url);
        const agents = await resp.json();
        const items = Array.isArray(agents) ? agents : [];
        if (items.length === 0) {
            list.innerHTML = "<p class=\"loading\">No agents found.</p>";
            return;
        }
        list.innerHTML = items.map(a => `
            <div class="agent-card" onclick="window.location.href=\`${BASE}/miniapp/agent/${a.id}\`">
                <h3>${esc(a.name)}</h3>
                <p>${esc(a.description || "")}</p>
                <div class="meta">
                    ${a.framework ? `<span>${esc(a.framework)}</span>` : ""}
                    ${a.tx_hash ? "<span>On-Chain</span>" : ""}
                </div>
            </div>
        `).join("");
    } catch(e) {
        list.innerHTML = "<p class=\"loading error\">Failed to load agents.</p>";
    }
}

window.startScan = async function() {
    const url = document.getElementById("scan-url").value.trim();
    if (!url) return;
    const btn = document.getElementById("scan-btn");
    const result = document.getElementById("scan-result");
    btn.disabled = true;
    btn.textContent = "Scanning...";
    result.textContent = "Scanning repository...";
    try {
        const resp = await fetch(`${BASE}/generate`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ github_url: url })
        });
        if (!resp.ok) throw new Error(await resp.text());
        const data = await resp.json();
        result.textContent = `Agent: ${data.manifest.name}\nDescription: ${data.manifest.description}\nCapabilities: ${data.manifest.capabilities?.length || 0}\nEndpoints: ${data.manifest.endpoints?.length || 0}\n\n${data.agents_md.substring(0, 800)}`;
    } catch(e) {
        result.innerHTML = `<span class="error">Scan failed: ${esc(e.message)}</span>`;
    }
    btn.disabled = false;
    btn.textContent = "Scan";
};

function esc(s) { return s ? s.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;") : ""; }

loadAgents("");
