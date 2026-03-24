#![allow(dead_code)]

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use crate::db;

// ── Agent Health Monitoring ─────────────────────────────────────────
//
// Pings registered agents' endpoints to check liveness.
// Stores health status in Supabase so the registry shows online/offline.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HealthStatus {
    pub agent_id: String,
    pub status: AgentStatus,
    pub latency_ms: Option<u64>,
    pub last_checked: String,
    pub endpoint: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Online,
    Offline,
    Degraded,
    Unknown,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Online => write!(f, "online"),
            AgentStatus::Offline => write!(f, "offline"),
            AgentStatus::Degraded => write!(f, "degraded"),
            AgentStatus::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct HealthQuery {
    pub agent_id: Option<String>,
    pub status: Option<String>,
}

pub struct HealthMonitor;

impl HealthMonitor {
    /// Ping a single agent's endpoint and return its health status
    pub async fn check_agent(agent_id: &str) -> Result<HealthStatus> {
        // Get the agent's registered endpoint
        let endpoint = db::get_a2a_endpoint(agent_id).await?;

        let now = chrono::Utc::now().to_rfc3339();

        let Some(url) = endpoint else {
            let status = HealthStatus {
                agent_id: agent_id.to_string(),
                status: AgentStatus::Unknown,
                latency_ms: None,
                last_checked: now,
                endpoint: None,
                error: Some("No endpoint registered".to_string()),
            };
            db::upsert_health_status(&status).await?;
            return Ok(status);
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let start = std::time::Instant::now();
        let result = client.get(&url).send().await;
        let latency = start.elapsed().as_millis() as u64;

        let (agent_status, error) = match result {
            Ok(resp) => {
                if resp.status().is_success() {
                    if latency > 5000 {
                        (AgentStatus::Degraded, None)
                    } else {
                        (AgentStatus::Online, None)
                    }
                } else {
                    (AgentStatus::Degraded, Some(format!("HTTP {}", resp.status())))
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    (AgentStatus::Offline, Some("Connection timed out".to_string()))
                } else {
                    (AgentStatus::Offline, Some(e.to_string()))
                }
            }
        };

        let status = HealthStatus {
            agent_id: agent_id.to_string(),
            status: agent_status,
            latency_ms: Some(latency),
            last_checked: now,
            endpoint: Some(url),
            error,
        };

        db::upsert_health_status(&status).await?;

        Ok(status)
    }

    /// Get the stored health status for an agent
    pub async fn get_status(agent_id: &str) -> Result<Option<HealthStatus>> {
        db::get_health_status(agent_id).await
    }

    /// Get health statuses filtered by status
    pub async fn list_statuses(status_filter: Option<&str>, limit: usize) -> Result<Vec<HealthStatus>> {
        db::list_health_statuses(status_filter, limit).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Online.to_string(), "online");
        assert_eq!(AgentStatus::Offline.to_string(), "offline");
        assert_eq!(AgentStatus::Degraded.to_string(), "degraded");
        assert_eq!(AgentStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus {
            agent_id: "agent-123".to_string(),
            status: AgentStatus::Online,
            latency_ms: Some(42),
            last_checked: "2026-01-01T00:00:00Z".to_string(),
            endpoint: Some("https://example.com/health".to_string()),
            error: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["status"], "online");
        assert_eq!(json["latency_ms"], 42);
        assert!(json["error"].is_null());
    }

    #[test]
    fn test_health_status_with_error() {
        let status = HealthStatus {
            agent_id: "agent-456".to_string(),
            status: AgentStatus::Offline,
            latency_ms: None,
            last_checked: "2026-01-01T00:00:00Z".to_string(),
            endpoint: None,
            error: Some("Connection refused".to_string()),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["status"], "offline");
        assert_eq!(json["error"], "Connection refused");
    }

    #[test]
    fn test_degraded_status_serialization() {
        let status = HealthStatus {
            agent_id: "agent-789".to_string(),
            status: AgentStatus::Degraded,
            latency_ms: Some(6000),
            last_checked: "2026-01-01T00:00:00Z".to_string(),
            endpoint: Some("https://slow-agent.com".to_string()),
            error: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["status"], "degraded");
        assert_eq!(json["latency_ms"], 6000);
    }
}
