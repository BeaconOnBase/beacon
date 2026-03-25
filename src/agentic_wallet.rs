use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Coinbase Agentic Wallet Integration
///
/// Provisions non-custodial wallets for registered agents via the
/// Coinbase Developer Platform (CDP) REST API.
///
/// Required env vars:
///   CDP_API_KEY_ID     — CDP API key ID
///   CDP_API_KEY_SECRET — CDP API key secret
///
/// Docs: https://docs.cdp.coinbase.com/agent-kit/core-concepts/wallet-management

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticWallet {
    pub agent_id: String,
    pub wallet_address: String,
    pub wallet_id: String,
    pub chain: String,
    pub network: String,
}

/// CDP wallet creation response shape
#[derive(Debug, Deserialize)]
struct CdpWalletResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    default_address: Option<CdpAddress>,
}

#[derive(Debug, Deserialize)]
struct CdpAddress {
    #[serde(default)]
    address_id: Option<String>,
}

/// CDP API configuration
struct CdpConfig {
    api_key_id: String,
    api_key_secret: String,
    base_url: String,
    network_id: String,
}

impl CdpConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            api_key_id: std::env::var("CDP_API_KEY_ID")
                .context("CDP_API_KEY_ID not set")?,
            api_key_secret: std::env::var("CDP_API_KEY_SECRET")
                .context("CDP_API_KEY_SECRET not set")?,
            base_url: std::env::var("CDP_API_URL")
                .unwrap_or_else(|_| "https://api.developer.coinbase.com".to_string()),
            network_id: std::env::var("CDP_NETWORK_ID")
                .unwrap_or_else(|_| "base-mainnet".to_string()),
        })
    }
}

/// Provision a new Agentic Wallet for an agent via CDP REST API
pub async fn provision_wallet(agent_id: &str) -> Result<AgenticWallet> {
    let config = CdpConfig::from_env()?;

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .build()?;

    // Create a wallet via CDP API
    let resp = client
        .post(format!("{}/platform/v1/wallets", config.base_url))
        .header("Content-Type", "application/json")
        .header("X-Api-Key-Id", &config.api_key_id)
        .header("X-Api-Key-Secret", &config.api_key_secret)
        .json(&serde_json::json!({
            "network_id": config.network_id,
            "idempotency_key": format!("beacon-agent-{}", agent_id),
        }))
        .send()
        .await
        .context("Failed to call CDP API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("CDP API returned {}: {}", status, body);
    }

    let wallet_resp: CdpWalletResponse = resp.json().await
        .context("Failed to parse CDP wallet response")?;

    let wallet_id = wallet_resp.id.unwrap_or_default();
    let wallet_address = wallet_resp
        .default_address
        .and_then(|a| a.address_id)
        .unwrap_or_default();

    if wallet_address.is_empty() {
        anyhow::bail!("CDP API returned empty wallet address");
    }

    Ok(AgenticWallet {
        agent_id: agent_id.to_string(),
        wallet_address,
        wallet_id,
        chain: "base".to_string(),
        network: config.network_id,
    })
}

/// Get wallet info for an agent from the database
pub async fn get_wallet(agent_id: &str) -> Result<Option<AgenticWallet>> {
    let agent = crate::db::get_registry_agent(agent_id).await?;

    match agent {
        Some(entry) if entry.wallet_address.is_some() => {
            Ok(Some(AgenticWallet {
                agent_id: agent_id.to_string(),
                wallet_address: entry.wallet_address.unwrap_or_default(),
                wallet_id: String::new(),
                chain: "base".to_string(),
                network: "base-mainnet".to_string(),
            }))
        }
        _ => Ok(None),
    }
}
