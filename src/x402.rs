#![allow(dead_code)]

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

// ── x402 Payment Protocol ──────────────────────────────────────────
//
// Integration with the x402 payment protocol for HTTP 402-based
// micropayments on Base. Agents can gate endpoints behind USDC
// payments verified through a facilitator service.

#[derive(Debug, Clone)]
pub struct X402Config {
    pub facilitator_url: String,
    pub receiver_address: String,
    pub chain_id: u64,
}

impl X402Config {
    pub fn from_env() -> Self {
        Self {
            facilitator_url: std::env::var("X402_FACILITATOR_URL")
                .unwrap_or_else(|_| "https://x402.org/facilitator".to_string()),
            receiver_address: std::env::var("X402_RECEIVER_ADDRESS")
                .unwrap_or_default(),
            chain_id: std::env::var("X402_CHAIN_ID")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8453), // Base mainnet
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaymentPayload {
    #[serde(default)]
    pub x402_version: Option<String>,
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaymentRequirements {
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(rename = "maxAmountRequired", default)]
    pub max_amount_required: Option<String>,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "mimeType", default)]
    pub mime_type: Option<String>,
    #[serde(rename = "payTo", default)]
    pub pay_to: Option<String>,
    #[serde(rename = "maxTimeoutSeconds", default)]
    pub max_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub extra: Option<serde_json::Value>,
}

/// Convert a USDC amount string (e.g. "0.09") to atomic units (6 decimals)
pub fn usdc_to_atomic(amount: &str) -> Result<String> {
    let parsed: f64 = amount.parse()
        .context("Invalid USDC amount")?;
    let atomic = (parsed * 1_000_000.0) as u64;
    Ok(atomic.to_string())
}

/// Build payment requirements for a given resource
pub fn build_payment_requirements(
    resource: &str,
    atomic_amount: &str,
    config: &X402Config,
) -> PaymentRequirements {
    PaymentRequirements {
        scheme: Some("exact".to_string()),
        network: Some(format!("eip155:{}", config.chain_id)),
        max_amount_required: Some(atomic_amount.to_string()),
        resource: Some(resource.to_string()),
        description: Some(format!("Payment for {}", resource)),
        mime_type: Some("application/json".to_string()),
        pay_to: Some(config.receiver_address.clone()),
        max_timeout_seconds: Some(300),
        extra: None,
    }
}

/// Verify a payment through the facilitator
pub async fn verify_payment(
    facilitator_url: &str,
    payload: &PaymentPayload,
    requirements: &PaymentRequirements,
) -> Result<bool> {
    let client = reqwest::Client::new();
    let resp = client.post(format!("{}/verify", facilitator_url))
        .json(&serde_json::json!({
            "paymentPayload": payload,
            "paymentRequirements": requirements,
        }))
        .send()
        .await
        .context("Failed to reach x402 facilitator")?;

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await?;
        Ok(body.get("valid").and_then(|v| v.as_bool()).unwrap_or(false))
    } else {
        Ok(false)
    }
}

/// Settle a payment through the facilitator
pub async fn settle_payment(
    facilitator_url: &str,
    payload: &PaymentPayload,
    requirements: &PaymentRequirements,
) -> Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let resp = client.post(format!("{}/settle", facilitator_url))
        .json(&serde_json::json!({
            "paymentPayload": payload,
            "paymentRequirements": requirements,
        }))
        .send()
        .await
        .context("Failed to reach x402 facilitator for settlement")?;

    let body: serde_json::Value = resp.json().await
        .context("Invalid response from facilitator")?;
    Ok(body)
}
