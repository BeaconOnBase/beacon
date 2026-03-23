use anyhow::{Context, Result};
use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::IntoResponse,
};

use crate::db;

/// Generates an SVG string for an agent card, then rasterizes it to PNG via resvg.
pub async fn handle_og_image(
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
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

    let png_bytes = render_agent_card(
        &agent.name,
        &truncate(&agent.description, 90),
        caps_count,
        endpoints_count,
        verified,
        framework,
    )
    .map_err(|e| {
        tracing::error!("OG image render failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        png_bytes,
    ))
}

/// Renders a default Beacon OG card (not agent-specific).
pub async fn handle_og_default() -> Result<impl IntoResponse, StatusCode> {
    let png_bytes = render_agent_card(
        "Beacon",
        "The Verifiable Agentic Protocol on Base",
        0,
        0,
        false,
        "",
    )
    .map_err(|e| {
        tracing::error!("Default OG image render failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        png_bytes,
    ))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_agent_card(
    name: &str,
    description: &str,
    caps: usize,
    endpoints: usize,
    verified: bool,
    framework: &str,
) -> Result<Vec<u8>> {
    let badge = if verified {
        r##"<rect x="900" y="40" width="260" height="40" rx="20" fill="#22c55e"/>
        <text x="1030" y="66" font-family="sans-serif" font-size="18" fill="white" text-anchor="middle" font-weight="bold">VERIFIED ON-CHAIN</text>"##
    } else {
        ""
    };

    let stats_section = if caps > 0 || endpoints > 0 {
        format!(
            r##"<rect x="60" y="310" width="180" height="80" rx="12" fill="#1a1a2e"/>
            <text x="150" y="348" font-family="sans-serif" font-size="36" fill="white" text-anchor="middle" font-weight="bold">{}</text>
            <text x="150" y="375" font-family="sans-serif" font-size="14" fill="#94a3b8" text-anchor="middle">Capabilities</text>
            <rect x="270" y="310" width="180" height="80" rx="12" fill="#1a1a2e"/>
            <text x="360" y="348" font-family="sans-serif" font-size="36" fill="white" text-anchor="middle" font-weight="bold">{}</text>
            <text x="360" y="375" font-family="sans-serif" font-size="14" fill="#94a3b8" text-anchor="middle">Endpoints</text>"##,
            caps, endpoints
        )
    } else {
        String::new()
    };

    let framework_section = if !framework.is_empty() {
        format!(
            r##"<rect x="480" y="310" width="200" height="80" rx="12" fill="#1a1a2e"/>
            <text x="580" y="348" font-family="sans-serif" font-size="20" fill="white" text-anchor="middle" font-weight="bold">{}</text>
            <text x="580" y="375" font-family="sans-serif" font-size="14" fill="#94a3b8" text-anchor="middle">Framework</text>"##,
            escape_xml(framework)
        )
    } else {
        String::new()
    };

    let svg = format!(
        r##"<svg width="1200" height="630" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1200" y2="630" gradientUnits="userSpaceOnUse">
      <stop offset="0%" stop-color="#0a0a0f"/>
      <stop offset="100%" stop-color="#111127"/>
    </linearGradient>
  </defs>
  <rect width="1200" height="630" fill="url(#bg)"/>
  <rect x="0" y="0" width="1200" height="4" fill="#6366f1"/>
  {}
  <text x="60" y="130" font-family="sans-serif" font-size="52" fill="white" font-weight="bold">{}</text>
  <text x="60" y="185" font-family="sans-serif" font-size="22" fill="#94a3b8">{}</text>
  <line x1="60" y1="230" x2="1140" y2="230" stroke="#1e293b" stroke-width="1"/>
  {}
  {}
  <rect x="60" y="530" width="140" height="44" rx="8" fill="#1a1a2e"/>
  <text x="82" y="558" font-family="sans-serif" font-size="20" fill="white" font-weight="bold">BEACON</text>
  <text x="1140" y="558" font-family="sans-serif" font-size="16" fill="#475569" text-anchor="end">beaconbase.xyz</text>
</svg>"##,
        badge,
        escape_xml(name),
        escape_xml(description),
        stats_section,
        framework_section,
    );

    svg_to_png(&svg)
}

fn svg_to_png(svg_str: &str) -> Result<Vec<u8>> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg_str, &options)
        .context("Failed to parse SVG")?;

    let size = tree.size();
    let width = size.width() as u32;
    let height = size.height() as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .context("Failed to create pixmap")?;

    resvg::render(&tree, resvg::usvg::Transform::default(), &mut pixmap.as_mut());

    pixmap.encode_png().context("Failed to encode PNG")
}
