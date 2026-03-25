#![allow(dead_code)]

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use crate::db;

// ── Export Agent Card ───────────────────────────────────────────────
//
// Export a registered agent's data as a standardized JSON-LD Agent Card
// or Google A2A-compatible format. Transforms existing manifest data
// into portable, shareable formats.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentCard {
    #[serde(rename = "@context")]
    pub context: String,
    #[serde(rename = "@type")]
    pub card_type: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub url: Option<String>,
    pub provider: Option<AgentProvider>,
    pub capabilities: Vec<ExportedCapability>,
    pub endpoints: Vec<ExportedEndpoint>,
    pub attestations: i64,
    pub health_status: Option<String>,
    pub tags: Vec<String>,
    pub ipfs_cid: Option<String>,
    pub registered_at: Option<String>,
    pub exported_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentProvider {
    pub address: String,
    pub basename: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportedCapability {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportedEndpoint {
    pub path: String,
    pub method: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>, // "json-ld" (default) or "a2a"
}

pub struct AgentExport;

impl AgentExport {
    /// Export an agent as a JSON-LD Agent Card
    pub async fn export_card(agent_id: &str) -> Result<AgentCard> {
        let agent = db::get_registry_agent(agent_id).await?
            .context("Agent not found")?;

        // Extract capabilities from manifest
        let manifest = &agent.manifest_json;
        let capabilities = manifest.get("capabilities").or_else(|| manifest.get("skills"))
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter().filter_map(|cap| {
                    let name = cap.get("name")?.as_str()?.to_string();
                    let desc = cap.get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(ExportedCapability { name, description: desc })
                }).collect()
            })
            .unwrap_or_default();

        // Extract endpoints from manifest
        let endpoints = manifest.get("endpoints")
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter().filter_map(|ep| {
                    let path = ep.get("path")?.as_str()?.to_string();
                    let method = ep.get("method")
                        .and_then(|m| m.as_str())
                        .unwrap_or("GET")
                        .to_string();
                    let desc = ep.get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(ExportedEndpoint { path, method, description: desc })
                }).collect()
            })
            .unwrap_or_default();

        // Get tags
        let agent_tags = db::get_agent_tags(agent_id).await
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.tag)
            .collect();

        // Get attestation count
        let attestations = db::get_attestations_for_agent(agent_id).await
            .map(|a| a.len() as i64)
            .unwrap_or(0);

        // Get health status
        let health_status = db::get_health_status(agent_id).await
            .ok()
            .flatten()
            .map(|h| h.status.to_string());

        Ok(AgentCard {
            context: "https://schema.org".to_string(),
            card_type: "SoftwareAgent".to_string(),
            id: agent.id.to_string(),
            name: agent.name,
            description: agent.description,
            version: manifest.get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            url: manifest.get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            provider: Some(AgentProvider {
                address: agent.owner_address.unwrap_or_default(),
                basename: agent.basename,
            }),
            capabilities,
            endpoints,
            attestations,
            health_status,
            tags: agent_tags,
            ipfs_cid: agent.manifest_cid,
            registered_at: agent.created_at,
            exported_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Export as A2A-compatible format
    pub async fn export_a2a(agent_id: &str) -> Result<serde_json::Value> {
        let card = Self::export_card(agent_id).await?;

        Ok(serde_json::json!({
            "protocolVersion": "1.0.0",
            "name": card.name,
            "description": card.description,
            "version": card.version,
            "url": card.url,
            "provider": card.provider.map(|p| serde_json::json!({
                "address": p.address,
                "basename": p.basename,
            })),
            "capabilities": {
                "streaming": false,
                "push_notifications": false,
            },
            "skills": card.capabilities.iter().map(|c| serde_json::json!({
                "name": c.name,
                "description": c.description,
            })).collect::<Vec<_>>(),
            "tags": card.tags,
            "attestations": card.attestations,
            "healthStatus": card.health_status,
            "ipfsCid": card.ipfs_cid,
        }))
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_serialization() {
        let card = AgentCard {
            context: "https://schema.org".into(),
            card_type: "SoftwareAgent".into(),
            id: "agent-1".into(),
            name: "SwapBot".into(),
            description: "Token swaps on Base".into(),
            version: Some("1.0.0".into()),
            url: Some("https://swap.example.com".into()),
            provider: Some(AgentProvider {
                address: "0x1234".into(),
                basename: Some("swapbot.base.eth".into()),
            }),
            capabilities: vec![
                ExportedCapability { name: "swap".into(), description: "Swap tokens".into() },
            ],
            endpoints: vec![
                ExportedEndpoint { path: "/swap".into(), method: "POST".into(), description: "Execute swap".into() },
            ],
            attestations: 5,
            health_status: Some("online".into()),
            tags: vec!["defi".into(), "swap".into()],
            ipfs_cid: Some("QmTest123".into()),
            registered_at: Some("2026-01-01T00:00:00Z".into()),
            exported_at: "2026-01-01T12:00:00Z".into(),
        };
        let json = serde_json::to_value(&card).unwrap();
        assert_eq!(json["@context"], "https://schema.org");
        assert_eq!(json["@type"], "SoftwareAgent");
        assert_eq!(json["name"], "SwapBot");
        assert_eq!(json["attestations"], 5);
        assert_eq!(json["tags"].as_array().unwrap().len(), 2);
        assert_eq!(json["capabilities"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_exported_capability_serialization() {
        let cap = ExportedCapability {
            name: "analyze".into(),
            description: "Analyze smart contracts".into(),
        };
        let json = serde_json::to_value(&cap).unwrap();
        assert_eq!(json["name"], "analyze");
    }

    #[test]
    fn test_agent_provider_serialization() {
        let provider = AgentProvider {
            address: "0xabcd".into(),
            basename: None,
        };
        let json = serde_json::to_value(&provider).unwrap();
        assert_eq!(json["address"], "0xabcd");
        assert!(json["basename"].is_null());
    }

    #[test]
    fn test_export_query_defaults() {
        let q: ExportQuery = serde_json::from_str(r#"{}"#).unwrap();
        assert!(q.format.is_none());
    }
}
