use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{Html, IntoResponse},
};
use serde_json::json;

use crate::db;

/// Serves the Farcaster app manifest at /.well-known/farcaster.json
pub async fn handle_farcaster_manifest() -> impl IntoResponse {
    let base_url = std::env::var("BEACON_BASE_URL")
        .unwrap_or_else(|_| "https://www.beaconcloud.org".to_string());

    let manifest_header = std::env::var("FARCASTER_MANIFEST_HEADER").unwrap_or_default();
    let manifest_payload = std::env::var("FARCASTER_MANIFEST_PAYLOAD").unwrap_or_default();
    let manifest_signature = std::env::var("FARCASTER_MANIFEST_SIGNATURE").unwrap_or_default();

    let manifest = json!({
        "accountAssociation": {
            "header": manifest_header,
            "payload": manifest_payload,
            "signature": manifest_signature,
        },
        "frame": {
            "version": "1",
            "name": "Beacon",
            "iconUrl": format!("{}/banner.png", base_url),
            "homeUrl": format!("{}/miniapp", base_url),
            "splashBackgroundColor": "#0a0a0f",
            "webhookUrl": format!("{}/api/farcaster/webhook", base_url),
        }
    });

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
}

/// Serves the Mini App home page with fc:frame meta tags
pub async fn handle_miniapp_home() -> impl IntoResponse {
    let base_url = std::env::var("BEACON_BASE_URL")
        .unwrap_or_else(|_| "https://www.beaconcloud.org".to_string());

    let fc_frame = serde_json::json!({
        "version": "next",
        "imageUrl": format!("{}/banner.png", base_url),
        "button": {
            "title": "Open Beacon",
            "action": {
                "type": "launch_frame",
                "name": "Beacon",
                "url": format!("{}/miniapp", base_url),
                "splashBackgroundColor": "#0a0a0f"
            }
        }
    });

    let html = [
        "<!DOCTYPE html><html lang=\"en\"><head>",
        "<meta charset=\"utf-8\"/>",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>",
        "<title>Beacon - Agent Registry</title>",
        &format!("<meta property=\"og:title\" content=\"Beacon - The Verifiable Agentic Protocol\"/>"),
        "<meta property=\"og:description\" content=\"Browse, scan, and register AI agents on Base.\"/>",
        &format!("<meta property=\"og:image\" content=\"{}/banner.png\"/>", base_url),
        &format!("<meta property=\"fc:frame\" content='{}' />", fc_frame),
        MINIAPP_STYLES,
        "</head><body><div id=\"app\">",
        "<header><h1>BEACON</h1><p class=\"subtitle\">The Verifiable Agentic Protocol on Base</p></header>",
        "<nav>",
        "<button class=\"tab active\" data-tab=\"browse\" onclick=\"switchTab(&quot;browse&quot;)\">Browse Agents</button>",
        "<button class=\"tab\" data-tab=\"scan\" onclick=\"switchTab(&quot;scan&quot;)\">Scan Repo</button>",
        "</nav><main>",
        "<section id=\"browse\" class=\"panel active\">",
        "<div class=\"search-bar\"><input type=\"text\" id=\"search-input\" placeholder=\"Search agents...\" oninput=\"searchAgents()\"/></div>",
        "<div id=\"agents-list\" class=\"agents-grid\"><p class=\"loading\">Loading agents...</p></div>",
        "</section>",
        "<section id=\"scan\" class=\"panel\">",
        "<div class=\"scan-form\"><input type=\"text\" id=\"scan-url\" placeholder=\"https://github.com/user/repo\"/>",
        "<button onclick=\"startScan()\" id=\"scan-btn\">Scan</button></div>",
        "<div id=\"scan-result\" class=\"result-box\"></div>",
        "</section></main></div>",
        &miniapp_script(&base_url),
        "</body></html>",
    ].join("\n");

    Html(html)
}

/// Serves an agent detail page with agent-specific OG tags
pub async fn handle_miniapp_agent(
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let base_url = std::env::var("BEACON_BASE_URL")
        .unwrap_or_else(|_| "https://www.beaconcloud.org".to_string());

    let agent = db::get_registry_agent(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let caps_count = agent
        .manifest_json
        .as_ref()
        .and_then(|m| m.get("capabilities"))
        .and_then(|c| c.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let endpoints_count = agent
        .manifest_json
        .as_ref()
        .and_then(|m| m.get("endpoints"))
        .and_then(|e| e.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let verified = agent.tx_hash.is_some();
    let framework = agent.framework.as_deref().unwrap_or("Unknown");

    let caps_html = agent
        .manifest_json
        .as_ref()
        .and_then(|m| m.get("capabilities"))
        .and_then(|c| c.as_array())
        .map(|caps| {
            caps.iter()
                .filter_map(|c| {
                    let name = c.get("name")?.as_str()?;
                    let desc = c.get("description")?.as_str()?;
                    Some(format!(
                        r#"<div class="cap-item"><code>{}</code><span>{}</span></div>"#,
                        escape_html(name),
                        escape_html(desc)
                    ))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    let endpoints_html = agent
        .manifest_json
        .as_ref()
        .and_then(|m| m.get("endpoints"))
        .and_then(|e| e.as_array())
        .map(|eps| {
            eps.iter()
                .filter_map(|e| {
                    let path = e.get("path")?.as_str()?;
                    let method = e.get("method")?.as_str()?;
                    let desc = e.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    Some(format!(
                        r#"<div class="ep-item"><span class="method">{}</span><code>{}</code><span>{}</span></div>"#,
                        escape_html(method),
                        escape_html(path),
                        escape_html(desc)
                    ))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    let name_escaped = escape_html(&agent.name);
    let desc_escaped = escape_html(&agent.description);

    let fc_frame = serde_json::json!({
        "version": "next",
        "imageUrl": format!("{}/og/agent/{}", base_url, id),
        "button": {
            "title": "View Agent",
            "action": {
                "type": "launch_frame",
                "name": "Beacon",
                "url": format!("{}/miniapp/agent/{}", base_url, id),
                "splashBackgroundColor": "#0a0a0f"
            }
        }
    });

    let verified_badge = if verified {
        "<span class=\"badge verified\">Verified On-Chain</span>"
    } else {
        ""
    };
    let framework_badge = format!("<span class=\"badge framework\">{}</span>", escape_html(framework));

    let caps_section = if !caps_html.is_empty() {
        format!("<h3 class=\"section-title\">Capabilities</h3>{}", caps_html)
    } else {
        String::new()
    };
    let endpoints_section = if !endpoints_html.is_empty() {
        format!("<h3 class=\"section-title\">Endpoints</h3>{}", endpoints_html)
    } else {
        String::new()
    };

    let agent_styles = concat!(
        "<style>",
        ".agent-header { margin-bottom: 24px; }",
        ".agent-header h2 { font-size: 28px; margin: 0 0 8px 0; }",
        ".agent-header .desc { color: #94a3b8; font-size: 16px; }",
        ".badge { display: inline-block; padding: 4px 12px; border-radius: 12px; font-size: 12px; font-weight: 600; }",
        ".badge.verified { background: #22c55e; color: white; }",
        ".badge.framework { background: #6366f1; color: white; }",
        ".stats-row { display: flex; gap: 16px; margin: 16px 0; }",
        ".stat { background: #1a1a2e; padding: 12px 20px; border-radius: 8px; text-align: center; }",
        ".stat .num { font-size: 24px; font-weight: bold; }",
        ".stat .label { font-size: 12px; color: #94a3b8; }",
        ".section-title { font-size: 18px; margin: 24px 0 12px; color: #e2e8f0; }",
        ".cap-item, .ep-item { background: #1a1a2e; padding: 10px 14px; border-radius: 6px; margin-bottom: 6px; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }",
        ".cap-item code, .ep-item code { color: #6366f1; font-size: 14px; }",
        ".ep-item .method { background: #6366f1; color: white; padding: 2px 8px; border-radius: 4px; font-size: 12px; font-weight: bold; }",
        ".back-link { color: #6366f1; text-decoration: none; font-size: 14px; display: inline-block; margin-bottom: 16px; }",
        "</style>",
    );

    let html = [
        "<!DOCTYPE html><html lang=\"en\"><head>",
        "<meta charset=\"utf-8\"/>",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>",
        &format!("<title>{} - Beacon</title>", name_escaped),
        &format!("<meta property=\"og:title\" content=\"{} - Beacon Agent\"/>", name_escaped),
        &format!("<meta property=\"og:description\" content=\"{}\"/>", desc_escaped),
        &format!("<meta property=\"og:image\" content=\"{}/og/agent/{}\"/>", base_url, id),
        &format!("<meta property=\"fc:frame\" content='{}' />", fc_frame),
        MINIAPP_STYLES,
        agent_styles,
        "</head><body><div id=\"app\">",
        "<header><h1>BEACON</h1></header><main>",
        &format!("<a href=\"{}/miniapp\" class=\"back-link\">&larr; Back to Registry</a>", base_url),
        &format!("<div class=\"agent-header\"><h2>{}</h2><p class=\"desc\">{}</p>{}{}</div>",
            name_escaped, desc_escaped, verified_badge, framework_badge),
        &format!("<div class=\"stats-row\"><div class=\"stat\"><div class=\"num\">{}</div><div class=\"label\">Capabilities</div></div><div class=\"stat\"><div class=\"num\">{}</div><div class=\"label\">Endpoints</div></div></div>",
            caps_count, endpoints_count),
        &caps_section,
        &endpoints_section,
        "</main></div>",
        "<script type=\"module\">import sdk from \"https://esm.sh/@farcaster/frame-sdk\";sdk.actions.ready();</script>",
        "</body></html>",
    ].join("\n");

    Ok(Html(html))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}

const MINIAPP_STYLES: &str = r#"<style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { background: #0a0a0f; color: #e2e8f0; font-family: -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif; }
    #app { max-width: 600px; margin: 0 auto; padding: 16px; }
    header { text-align: center; padding: 20px 0 12px; }
    header h1 { font-size: 24px; letter-spacing: 4px; color: white; }
    header .subtitle { color: #64748b; font-size: 13px; margin-top: 4px; }
    nav { display: flex; gap: 8px; margin-bottom: 16px; }
    .tab { flex: 1; padding: 10px; background: #1a1a2e; border: 1px solid #1e293b; border-radius: 8px; color: #94a3b8; font-size: 14px; cursor: pointer; font-weight: 500; }
    .tab.active { background: #6366f1; color: white; border-color: #6366f1; }
    .panel { display: none; }
    .panel.active { display: block; }
    .search-bar { margin-bottom: 12px; }
    .search-bar input, .scan-form input { width: 100%; padding: 12px 16px; background: #1a1a2e; border: 1px solid #1e293b; border-radius: 8px; color: white; font-size: 14px; outline: none; }
    .search-bar input:focus, .scan-form input:focus { border-color: #6366f1; }
    .scan-form { display: flex; gap: 8px; margin-bottom: 16px; }
    .scan-form input { flex: 1; }
    .scan-form button { padding: 12px 24px; background: #6366f1; color: white; border: none; border-radius: 8px; font-weight: 600; cursor: pointer; font-size: 14px; white-space: nowrap; }
    .scan-form button:disabled { opacity: 0.5; cursor: not-allowed; }
    .agents-grid { display: flex; flex-direction: column; gap: 8px; }
    .agent-card { background: #1a1a2e; border: 1px solid #1e293b; border-radius: 10px; padding: 14px; cursor: pointer; transition: border-color 0.2s; }
    .agent-card:hover { border-color: #6366f1; }
    .agent-card h3 { font-size: 16px; color: white; margin-bottom: 4px; }
    .agent-card p { font-size: 13px; color: #94a3b8; line-height: 1.4; }
    .agent-card .meta { display: flex; gap: 12px; margin-top: 8px; font-size: 12px; color: #64748b; }
    .result-box { background: #1a1a2e; border: 1px solid #1e293b; border-radius: 10px; padding: 16px; min-height: 100px; white-space: pre-wrap; font-size: 13px; line-height: 1.5; }
    .result-box:empty { display: none; }
    .loading { color: #64748b; text-align: center; padding: 20px; }
    .error { color: #ef4444; }
</style>"#;

const MINIAPP_JS: &str = include_str!("../../assets/miniapp.js");

fn miniapp_script(base_url: &str) -> String {
    let js = MINIAPP_JS.replace("%%BASE_URL%%", base_url);
    let mut s = String::from("<script type=\"module\">\n");
    s.push_str(&js);
    s.push_str("\n</script>");
    s
}
