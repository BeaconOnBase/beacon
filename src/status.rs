#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db;

// ── Agent Status Page ───────────────────────────────────────────────
//
// Public summary endpoint: total agents, online/offline counts,
// top tags, recent registrations. One-stop overview of the registry.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryStatus {
    pub total_agents: i64,
    pub agents_online: i64,
    pub agents_offline: i64,
    pub agents_degraded: i64,
    pub agents_unknown: i64,
    pub total_attestations: i64,
    pub total_messages: i64,
    pub top_tags: Vec<String>,
    pub recent_agents: Vec<RecentAgent>,
    pub generated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecentAgent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub registered_at: Option<String>,
}

pub struct StatusPage;

impl StatusPage {
    /// Build a full registry status summary
    pub async fn get_status() -> Result<RegistryStatus> {
        let counts = db::get_registry_counts().await?;
        let health_counts = db::get_health_counts().await?;
        let top_tags = db::get_top_tag_names(5).await?;
        let recent = db::get_recent_agents(5).await?;

        Ok(RegistryStatus {
            total_agents: counts.total,
            agents_online: health_counts.online,
            agents_offline: health_counts.offline,
            agents_degraded: health_counts.degraded,
            agents_unknown: counts.total - health_counts.online - health_counts.offline - health_counts.degraded,
            total_attestations: counts.attestations,
            total_messages: counts.messages,
            top_tags,
            recent_agents: recent,
            generated_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_status_serialization() {
        let status = RegistryStatus {
            total_agents: 42,
            agents_online: 30,
            agents_offline: 5,
            agents_degraded: 2,
            agents_unknown: 5,
            total_attestations: 120,
            total_messages: 500,
            top_tags: vec!["defi".into(), "nft".into(), "security".into()],
            recent_agents: vec![
                RecentAgent {
                    id: "agent-1".into(),
                    name: "SwapBot".into(),
                    description: "Token swaps".into(),
                    registered_at: Some("2026-01-01T00:00:00Z".into()),
                },
            ],
            generated_at: "2026-01-01T12:00:00Z".into(),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["total_agents"], 42);
        assert_eq!(json["agents_online"], 30);
        assert_eq!(json["top_tags"].as_array().unwrap().len(), 3);
        assert_eq!(json["recent_agents"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_recent_agent_serialization() {
        let agent = RecentAgent {
            id: "agent-123".into(),
            name: "TestAgent".into(),
            description: "A test".into(),
            registered_at: None,
        };
        let json = serde_json::to_value(&agent).unwrap();
        assert_eq!(json["name"], "TestAgent");
        assert!(json["registered_at"].is_null());
    }

    #[test]
    fn test_unknown_count_calculation() {
        let total = 100i64;
        let online = 60i64;
        let offline = 20i64;
        let degraded = 5i64;
        let unknown = total - online - offline - degraded;
        assert_eq!(unknown, 15);
    }
}
