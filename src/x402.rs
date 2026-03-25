use anyhow::Result;
use serde::{Deserialize, Serialize};

/// x402 Payment Protocol Support
///
/// Implements the x402 HTTP payment standard for Beacon endpoints.
/// This module provides:
/// - Payment requirement generation (402 responses)
/// - Payment verification via facilitator
/// - Endpoint pricing configuration
///
/// Spec: https://x402.org

// ── Payment Requirement (server → client) ───────────────────────────

/// Payment requirements returned in the PAYMENT-REQUIRED header
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRequirements {
    pub version: String,
    pub scheme: String,
    pub network: String,
    pub asset: String,
    pub pay_to: String,
    pub amount: String,
    pub max_timeout_seconds: u64,
    pub resource: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// Payment payload sent by the client in PAYMENT-SIGNATURE header
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentPayload {
    pub x402_version: u32,
    pub scheme: String,
    pub network: String,
    pub payload: serde_json::Value,
}

/// Facilitator verify/settle response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacilitatorResponse {
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub transaction: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub payer: Option<String>,
}

/// Payment response returned in PAYMENT-RESPONSE header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentResponse {
    pub success: bool,
    #[serde(default)]
    pub txs: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub payer: Option<String>,
}

// ── Endpoint Pricing ────────────────────────────────────────────────

/// x402 pricing information for an agent endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointPricing {
    pub x402_enabled: bool,
    pub price_per_call: Option<String>,
    pub payment_currency: Option<String>,
    pub payment_network: Option<String>,
    pub pay_to: Option<String>,
}

impl Default for EndpointPricing {
    fn default() -> Self {
        Self {
            x402_enabled: false,
            price_per_call: None,
            payment_currency: None,
            payment_network: None,
            pay_to: None,
        }
    }
}

// ── Configuration ───────────────────────────────────────────────────

/// Base USDC contract on Base mainnet
pub const BASE_USDC: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
/// Base Sepolia USDC (for testing)
pub const BASE_SEPOLIA_USDC: &str = "0x036CbD53842c5426634e7929541eC2318f3dCF7e";

/// x402 configuration
pub struct X402Config {
    pub facilitator_url: String,
    pub pay_to: String,
    pub network: String,
    pub asset: String,
}

impl X402Config {
    pub fn from_env() -> Self {
        Self {
            facilitator_url: std::env::var("X402_FACILITATOR_URL")
                .unwrap_or_else(|_| "https://x402.org/facilitator".to_string()),
            pay_to: std::env::var("BEACON_WALLET_BASE").unwrap_or_default(),
            network: std::env::var("X402_NETWORK")
                .unwrap_or_else(|_| "base".to_string()),
            asset: std::env::var("X402_ASSET")
                .unwrap_or_else(|_| BASE_USDC.to_string()),
        }
    }
}

// ── Core Functions ──────────────────────────────────────────────────

/// Build a PaymentRequirements for a given resource and price (in USDC atomic units)
pub fn build_payment_requirements(
    resource: &str,
    amount_usdc_atomic: &str,
    config: &X402Config,
) -> PaymentRequirements {
    PaymentRequirements {
        version: "v2".to_string(),
        scheme: "exact".to_string(),
        network: config.network.clone(),
        asset: config.asset.clone(),
        pay_to: config.pay_to.clone(),
        amount: amount_usdc_atomic.to_string(),
        max_timeout_seconds: 30,
        resource: resource.to_string(),
        mime_type: "application/json".to_string(),
        extra: serde_json::json!({}),
    }
}

/// Encode payment requirements as base64 for the PAYMENT-REQUIRED header
pub fn encode_payment_header(req: &PaymentRequirements) -> Result<String> {
    use base64::Engine;
    let json = serde_json::to_string(req)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(json.as_bytes()))
}

/// Decode a PAYMENT-SIGNATURE header from base64 JSON
pub fn decode_payment_signature(header: &str) -> Result<PaymentPayload> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(header)?;
    let payload: PaymentPayload = serde_json::from_slice(&bytes)?;
    Ok(payload)
}

/// Verify payment via the facilitator's /verify endpoint
pub async fn verify_payment(
    facilitator_url: &str,
    payment_payload: &PaymentPayload,
    payment_requirements: &PaymentRequirements,
) -> Result<bool> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .build()?;

    let body = serde_json::json!({
        "paymentPayload": payment_payload,
        "paymentRequirements": payment_requirements,
    });

    let resp = client
        .post(format!("{}/verify", facilitator_url))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let result: FacilitatorResponse = resp.json().await?;
    Ok(result.success)
}

/// Settle payment via the facilitator's /settle endpoint
pub async fn settle_payment(
    facilitator_url: &str,
    payment_payload: &PaymentPayload,
    payment_requirements: &PaymentRequirements,
) -> Result<FacilitatorResponse> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .build()?;

    let body = serde_json::json!({
        "paymentPayload": payment_payload,
        "paymentRequirements": payment_requirements,
    });

    let resp = client
        .post(format!("{}/settle", facilitator_url))
        .json(&body)
        .send()
        .await?;

    let result: FacilitatorResponse = resp.json().await?;
    Ok(result)
}

/// Convert a USDC human-readable amount (e.g., "0.09") to atomic units (e.g., "90000")
pub fn usdc_to_atomic(amount: &str) -> Result<String> {
    let parsed: f64 = amount.parse()?;
    let atomic = (parsed * 1_000_000.0) as u64;
    Ok(atomic.to_string())
}
