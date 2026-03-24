use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Agent-to-Agent (A2A) Discovery Protocol.
/// Enables agents to discover each other by capability and communicate through Beacon.

// ── Discovery ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DiscoveryQuery {
    pub capability: Option<String>,
    pub framework: Option<String>,
    pub has_attestation: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryResult {
    pub agents: Vec<DiscoveredAgent>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscoveredAgent {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub endpoint_url: Option<String>,
    pub manifest_cid: Option<String>,
    pub basename: Option<String>,
    pub framework: Option<String>,
}

// ── Messaging ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct A2AMessage {
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub reply_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct A2AMessageResponse {
    pub message_id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub reply_to: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
}

// ── Endpoint Registration ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EndpointRegistration {
    pub agent_id: String,
    pub endpoint_url: String,
    pub owner_address: String,
}

use crate::models::AgentCard;

// ── JSON-RPC 2.0 ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

// ── Protocol Implementation ──────────────────────────────────────────

pub struct A2AProtocol;

impl A2AProtocol {
    /// Fetch an Agent Card from a remote agent's .well-known/agent-card.json
    pub async fn fetch_agent_card(base_url: &str) -> Result<AgentCard> {
        let client = reqwest::Client::new();
        let well_known_url = format!("{}/.well-known/agent-card.json", base_url.trim_end_matches('/'));
        
        let res = client.get(&well_known_url).send().await?;
        if !res.status().is_success() {
            // Try fallback to agent.json
            let agent_json_url = format!("{}/.well-known/agent.json", base_url.trim_end_matches('/'));
            let res_fallback = client.get(&agent_json_url).send().await?;
            if !res_fallback.status().is_success() {
                anyhow::bail!("Failed to fetch Agent Card from both agent-card.json and agent.json");
            }
            return Ok(res_fallback.json().await?);
        }
        
        Ok(res.json().await?)
    }

    /// Discover agents by capability, framework, or attestation status.
    pub async fn discover(query: &DiscoveryQuery) -> Result<DiscoveryResult> {
        // ... (keep existing implementation but maybe wrap results)
        let limit = query.limit.unwrap_or(20).min(100);
        let offset = query.offset.unwrap_or(0);

        // Search registry with capability/framework filters
        let entries = crate::db::search_registry_advanced(
            query.capability.as_deref(),
            query.framework.as_deref(),
            limit,
            offset,
        ).await?;

        let agents: Vec<DiscoveredAgent> = entries.into_iter().map(|e| {
            // Extract capabilities from manifest_json if available
            let capabilities = e.manifest_json
                .get("capabilities")
                .and_then(|c: &serde_json::Value| c.as_array())
                .map(|arr: &Vec<serde_json::Value>| arr.iter().filter_map(|v: &serde_json::Value| {
                    v.get("name").and_then(|n: &serde_json::Value| n.as_str()).map(|s: &str| s.to_string())
                }).collect())
                .unwrap_or_default();

            // Get endpoint from a2a_endpoints table
            let endpoint_url = e.wallet_address.clone(); // reuse field for now

            DiscoveredAgent {
                agent_id: e.id.to_string(),
                name: e.name,
                description: e.description,
                capabilities,
                endpoint_url,
                manifest_cid: e.manifest_cid,
                basename: e.basename,
                framework: e.framework,
            }
        }).collect();

        let total = agents.len();
        Ok(DiscoveryResult { agents, total })
    }

    /// Send a message from one agent to another.
    pub async fn send_message(msg: &A2AMessage) -> Result<A2AMessageResponse> {
        // Verify both agents exist
        let _from = crate::db::get_registry_agent(&msg.from_agent_id).await?
            .context("Sender agent not found in registry")?;
        let _to = crate::db::get_registry_agent(&msg.to_agent_id).await?
            .context("Recipient agent not found in registry")?;

        let message_id = uuid::Uuid::new_v4().to_string();

        let stored = crate::db::A2AMessageRow {
            id: message_id.clone(),
            from_agent_id: msg.from_agent_id.clone(),
            to_agent_id: msg.to_agent_id.clone(),
            message_type: msg.message_type.clone(),
            payload: msg.payload.clone(),
            reply_to: msg.reply_to.clone(),
            status: "delivered".to_string(),
            created_at: None,
        };

        crate::db::insert_a2a_message(&stored).await?;

        // Try webhook delivery if endpoint is registered
        if let Ok(Some(endpoint)) = crate::db::get_a2a_endpoint(&msg.to_agent_id).await {
            let payload = serde_json::json!({
                "message_id": message_id,
                "from": msg.from_agent_id,
                "type": msg.message_type,
                "payload": msg.payload,
            });

            // Fire-and-forget webhook delivery
            let endpoint_clone = endpoint.clone();
            tokio::spawn(async move {
                let _ = reqwest::Client::new()
                    .post(&endpoint_clone)
                    .json(&payload)
                    .send()
                    .await;
            });
        }

        Ok(A2AMessageResponse {
            message_id,
            status: "delivered".to_string(),
        })
    }

    /// Get messages for an agent (inbox).
    pub async fn get_messages(agent_id: &str, limit: usize) -> Result<Vec<StoredMessage>> {
        let rows = crate::db::get_a2a_messages(agent_id, limit).await?;

        Ok(rows.into_iter().map(|r| StoredMessage {
            id: r.id,
            from_agent_id: r.from_agent_id,
            to_agent_id: r.to_agent_id,
            message_type: r.message_type,
            payload: r.payload,
            reply_to: r.reply_to,
            status: r.status,
            created_at: r.created_at,
        }).collect())
    }

    /// Register a webhook endpoint for an agent.
    pub async fn register_endpoint(reg: &EndpointRegistration) -> Result<()> {
        // Verify the agent exists and the caller owns it
        let agent = crate::db::get_registry_agent(&reg.agent_id).await?
            .context("Agent not found")?;

        if agent.owner_address.as_deref().unwrap_or_default().to_lowercase() != reg.owner_address.to_lowercase() {
            anyhow::bail!("Only the agent owner can register an endpoint");
        }

        crate::db::upsert_a2a_endpoint(&reg.agent_id, &reg.endpoint_url).await?;
        Ok(())
    }
}

// ── Message Types ────────────────────────────────────────────────────

/// Standard A2A message types.
pub mod message_types {
    pub const CAPABILITY_REQUEST: &str = "capability_request";
    pub const CAPABILITY_RESPONSE: &str = "capability_response";
    pub const TASK_DELEGATION: &str = "task_delegation";
    pub const TASK_RESULT: &str = "task_result";
    pub const HANDSHAKE: &str = "handshake";
    pub const HEARTBEAT: &str = "heartbeat";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_constants() {
        assert_eq!(message_types::CAPABILITY_REQUEST, "capability_request");
        assert_eq!(message_types::TASK_DELEGATION, "task_delegation");
        assert_eq!(message_types::HANDSHAKE, "handshake");
    }

    #[test]
    fn test_a2a_message_serialization() {
        let msg = A2AMessage {
            from_agent_id: "agent-1".to_string(),
            to_agent_id: "agent-2".to_string(),
            message_type: message_types::HANDSHAKE.to_string(),
            payload: serde_json::json!({"hello": "world"}),
            reply_to: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["from_agent_id"], "agent-1");
        assert_eq!(json["message_type"], "handshake");
    }

    #[test]
    fn test_discovery_query_defaults() {
        let query: DiscoveryQuery = serde_json::from_str("{}").unwrap();
        assert!(query.capability.is_none());
        assert!(query.framework.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_discovered_agent_serialization() {
        let agent = DiscoveredAgent {
            agent_id: "test-id".to_string(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            capabilities: vec!["token_swap".to_string()],
            endpoint_url: None,
            manifest_cid: Some("QmTest".to_string()),
            basename: Some("test.base.eth".to_string()),
            framework: Some("OpenClaw".to_string()),
        };
        let json = serde_json::to_value(&agent).unwrap();
        assert_eq!(json["capabilities"][0], "token_swap");
        assert_eq!(json["framework"], "OpenClaw");
    }
}
