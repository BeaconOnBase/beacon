#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db;

// ── Agent Analytics ─────────────────────────────────────────────────
//
// Tracks discovery counts, message volume, attestation count, and
// health check history per agent. Gives agent owners visibility
// into how their agent is being used.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnalyticsEvent {
    pub id: String,
    pub agent_id: String,
    pub event_type: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentStats {
    pub agent_id: String,
    pub total_discoveries: i64,
    pub total_messages_received: i64,
    pub total_messages_sent: i64,
    pub total_attestations: i64,
    pub total_health_checks: i64,
    pub last_active: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyticsQuery {
    pub agent_id: Option<String>,
    pub event_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// Event type constants
pub const EVENT_DISCOVERY: &str = "discovery";
pub const EVENT_MESSAGE_SENT: &str = "message_sent";
pub const EVENT_MESSAGE_RECEIVED: &str = "message_received";
pub const EVENT_ATTESTATION: &str = "attestation";
pub const EVENT_HEALTH_CHECK: &str = "health_check";
pub const EVENT_MANIFEST_PIN: &str = "manifest_pin";
pub const EVENT_REGISTRY_VIEW: &str = "registry_view";
pub const EVENT_MANIFEST_UPDATE: &str = "manifest_update";

pub struct AgentAnalytics;

impl AgentAnalytics {
    /// Record an analytics event
    pub async fn track(
        agent_id: &str,
        event_type: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let event = AnalyticsEvent {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            event_type: event_type.to_string(),
            metadata,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        db::insert_analytics_event(&event).await
    }

    /// Get aggregated stats for an agent
    pub async fn get_stats(agent_id: &str) -> Result<AgentStats> {
        db::get_agent_stats(agent_id).await
    }

    /// Get recent events for an agent
    pub async fn get_events(
        agent_id: &str,
        event_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AnalyticsEvent>> {
        db::get_analytics_events(agent_id, event_type, limit, offset).await
    }

    /// Get top agents by discovery count
    pub async fn get_trending(limit: usize) -> Result<Vec<AgentStats>> {
        db::get_trending_agents(limit).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_constants() {
        assert_eq!(EVENT_DISCOVERY, "discovery");
        assert_eq!(EVENT_MESSAGE_SENT, "message_sent");
        assert_eq!(EVENT_MESSAGE_RECEIVED, "message_received");
        assert_eq!(EVENT_ATTESTATION, "attestation");
        assert_eq!(EVENT_HEALTH_CHECK, "health_check");
        assert_eq!(EVENT_MANIFEST_PIN, "manifest_pin");
        assert_eq!(EVENT_REGISTRY_VIEW, "registry_view");
        assert_eq!(EVENT_MANIFEST_UPDATE, "manifest_update");
    }

    #[test]
    fn test_analytics_event_serialization() {
        let event = AnalyticsEvent {
            id: "evt-123".to_string(),
            agent_id: "agent-456".to_string(),
            event_type: EVENT_DISCOVERY.to_string(),
            metadata: Some(serde_json::json!({ "query": "defi" })),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event_type"], "discovery");
        assert_eq!(json["metadata"]["query"], "defi");
    }

    #[test]
    fn test_agent_stats_serialization() {
        let stats = AgentStats {
            agent_id: "agent-789".to_string(),
            total_discoveries: 150,
            total_messages_received: 42,
            total_messages_sent: 38,
            total_attestations: 3,
            total_health_checks: 200,
            last_active: Some("2026-01-01T12:00:00Z".to_string()),
        };
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["total_discoveries"], 150);
        assert_eq!(json["total_messages_received"], 42);
        assert_eq!(json["total_attestations"], 3);
    }

    #[test]
    fn test_agent_stats_default_zeros() {
        let stats = AgentStats {
            agent_id: "new-agent".to_string(),
            total_discoveries: 0,
            total_messages_received: 0,
            total_messages_sent: 0,
            total_attestations: 0,
            total_health_checks: 0,
            last_active: None,
        };
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["total_discoveries"], 0);
        assert!(json["last_active"].is_null());
    }
}
