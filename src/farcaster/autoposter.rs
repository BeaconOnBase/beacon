use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::time::{Duration, interval};
use reqwest::Client;
use once_cell::sync::Lazy;
use serde_json::{json, Value};

use crate::farcaster::neynar::NeynarClient;

static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .use_rustls_tls()
        .build()
        .expect("Failed to create reqwest client")
});

const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";

/// All Beacon features the AI can talk about when generating casts.
const BEACON_CONTEXT: &str = r#"
You are the social media voice for Beacon — The Verifiable Agentic Protocol on Base.

Here is what Beacon does (use ONLY these facts, never invent features):

1. REPOSITORY SCANNING: Beacon scans any codebase — extracts README, package manifests, OpenAPI specs, and source files. Detects agent frameworks like OpenClaw, AgentKit, LangChain, CrewAI, Eliza, and AutoGPT.

2. AI-POWERED INFERENCE: Uses Gemini, Claude, OpenAI, or DeepSeek to analyze repos and auto-detect capabilities, endpoints, and schemas. Generates structured AGENTS.md manifests.

3. AGENTS.MD GENERATION: Auto-generates standardized, AAIF-compliant agent manifest documentation with capabilities, endpoints, authentication, and rate limits.

4. AGENTS.MD VALIDATION: Schema validation with optional endpoint reachability checks. Reports errors and warnings.

5. ON-CHAIN IDENTITY: Registers repositories as ERC-7527 identity NFTs on Base Mainnet using a linear bonding curve. Early adopters get lower prices.

6. MCP SERVER: Native Model Context Protocol server — any LLM (Claude, Cursor, etc.) can discover and call Beacon tools automatically via SSE.

7. IPFS PINNING: Pins AGENTS.md manifests to IPFS via Pinata for permanent, decentralized, content-addressable storage.

8. AGENT REGISTRY: Searchable on-chain registry of agents by name, owner, or framework. Supports Base L2 Basename resolution.

9. FARCASTER BOT: Autonomous bot that scans and validates repos when mentioned on Farcaster. Broadcasts new agent registrations.

10. MULTI-CHAIN PAYMENTS: Supports USDC payments on Base and Solana via the x402 protocol. Pay-per-run verification.

11. CLI TOOL: Simple commands — `beacon generate`, `beacon validate`, `beacon register`, `beacon serve`, `beacon upgrade`.

12. BUILT IN RUST: Fast, memory-safe, compiled to static binaries. Runs anywhere.

13. OPEN SOURCE: Licensed under BUSL-1.1 on GitHub.
"#;

pub struct AutoPosterConfig {
    pub interval_secs: u64,
}

impl AutoPosterConfig {
    pub fn new(interval_secs: u64) -> Self {
        Self { interval_secs }
    }
}

/// Runs the auto-poster loop. Posts a new cast about Beacon tech every interval.
pub async fn run_autoposter(
    neynar: Arc<NeynarClient>,
    config: AutoPosterConfig,
) -> Result<()> {
    println!("📣 Beacon auto-poster starting — posting every {}s", config.interval_secs);
    let mut ticker = interval(Duration::from_secs(config.interval_secs));
    let mut recent_topics: Vec<String> = Vec::new();

    loop {
        ticker.tick().await;

        match generate_cast(&recent_topics).await {
            Ok(cast_text) => {
                let embeds = std::env::var("BEACON_BASE_URL").ok().map(|base| {
                    vec![format!("{}/miniapp", base)]
                });
                match neynar.post_cast_with_embeds(&cast_text, None, None, embeds).await {
                    Ok(hash) => {
                        tracing::info!("Auto-posted cast: {} ({})", &cast_text[..cast_text.len().min(50)], hash);
                        // Track recent topics to avoid repetition (keep last 20)
                        recent_topics.push(cast_text);
                        if recent_topics.len() > 20 {
                            recent_topics.remove(0);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to auto-post cast: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to generate cast content: {}", e);
            }
        }
    }
}

/// Uses Gemini to generate a single cast about Beacon's technology.
async fn generate_cast(recent_topics: &[String]) -> Result<String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .context("GEMINI_API_KEY not set")?;

    let recent_summary = if recent_topics.is_empty() {
        "None yet.".to_string()
    } else {
        recent_topics
            .iter()
            .rev()
            .take(10)
            .enumerate()
            .map(|(i, t)| format!("{}. {}", i + 1, t))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let prompt = format!(
        r#"{}

RECENT POSTS (do NOT repeat these topics or phrasings):
{}

Write a single Farcaster cast (max 280 characters) about one specific Beacon feature or capability.

Rules:
- Pick a DIFFERENT feature than the recent posts above
- Be specific and technical, not generic marketing fluff
- Sound like a builder sharing what they built, not an ad
- Use a casual, confident dev tone
- Can include one relevant emoji at most
- Do NOT use hashtags
- Do NOT start with "Beacon" every time — vary the opening
- Do NOT include links
- Return ONLY the cast text, nothing else
"#,
        BEACON_CONTEXT,
        recent_summary,
    );

    let response = CLIENT
        .post(format!("{}?key={}", GEMINI_URL, api_key))
        .json(&json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": {
                "temperature": 0.9,
                "maxOutputTokens": 200
            }
        }))
        .send()
        .await
        .context("Failed to reach Gemini API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Gemini API returned {}: {}", status, body);
    }

    let raw: Value = response.json().await?;

    // Check that Gemini actually finished generating
    let finish_reason = raw["candidates"][0]["finishReason"]
        .as_str()
        .unwrap_or("UNKNOWN");
    if finish_reason != "STOP" {
        anyhow::bail!(
            "Gemini did not finish generating (reason: {}), skipping cast",
            finish_reason
        );
    }

    let text = raw["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .context("Unexpected Gemini response shape")?
        .trim()
        .trim_matches('"')
        .to_string();

    // Reject obviously incomplete text (ends mid-sentence)
    if text.len() < 20 || (!text.ends_with(|c: char| ".!?…)\"'".contains(c)) && text.len() < 50) {
        anyhow::bail!("Generated cast looks incomplete: {:?}", text);
    }

    // Safety check: ensure it's within Farcaster's limit
    if text.len() > 1024 {
        Ok(text[..1024].to_string())
    } else {
        Ok(text)
    }
}
