#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db;

// ── Rate Limiting Tracker ───────────────────────────────────────────
//
// Tracks API usage per agent and per IP. Logs request counts over
// time windows (hour, day). Provides usage stats for agent owners
// and platform-wide visibility.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsageRecord {
    pub id: String,
    pub agent_id: Option<String>,
    pub ip_address: Option<String>,
    pub endpoint: String,
    pub method: String,
    pub window_start: String,
    pub request_count: i64,
    pub last_request: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentUsageSummary {
    pub agent_id: String,
    pub total_requests_24h: i64,
    pub total_requests_7d: i64,
    pub top_endpoints: Vec<EndpointUsage>,
    pub generated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EndpointUsage {
    pub endpoint: String,
    pub method: String,
    pub request_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlatformUsage {
    pub total_requests_24h: i64,
    pub total_requests_7d: i64,
    pub unique_agents_24h: i64,
    pub unique_ips_24h: i64,
    pub top_agents: Vec<AgentRequestCount>,
    pub top_endpoints: Vec<EndpointUsage>,
    pub generated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentRequestCount {
    pub agent_id: String,
    pub request_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    pub period: Option<String>, // "24h" or "7d"
}

pub struct RateLimitTracker;

impl RateLimitTracker {
    /// Record an API request
    pub async fn track_request(
        agent_id: Option<&str>,
        ip_address: Option<&str>,
        endpoint: &str,
        method: &str,
    ) -> Result<()> {
        let record = UsageRecord {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.map(|s| s.to_string()),
            ip_address: ip_address.map(|s| s.to_string()),
            endpoint: endpoint.to_string(),
            method: method.to_string(),
            window_start: Self::current_hour_window(),
            request_count: 1,
            last_request: Some(chrono::Utc::now().to_rfc3339()),
        };
        db::upsert_usage_record(&record).await
    }

    /// Get usage summary for a specific agent
    pub async fn get_agent_usage(agent_id: &str) -> Result<AgentUsageSummary> {
        let usage_24h = db::get_agent_request_count(agent_id, "24h").await?;
        let usage_7d = db::get_agent_request_count(agent_id, "7d").await?;
        let top_endpoints = db::get_agent_top_endpoints(agent_id, 5).await?;

        Ok(AgentUsageSummary {
            agent_id: agent_id.to_string(),
            total_requests_24h: usage_24h,
            total_requests_7d: usage_7d,
            top_endpoints,
            generated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Get platform-wide usage stats
    pub async fn get_platform_usage() -> Result<PlatformUsage> {
        let total_24h = db::get_platform_request_count("24h").await?;
        let total_7d = db::get_platform_request_count("7d").await?;
        let unique_agents = db::get_unique_agents_count("24h").await?;
        let unique_ips = db::get_unique_ips_count("24h").await?;
        let top_agents = db::get_top_agents_by_requests(5).await?;
        let top_endpoints = db::get_platform_top_endpoints(5).await?;

        Ok(PlatformUsage {
            total_requests_24h: total_24h,
            total_requests_7d: total_7d,
            unique_agents_24h: unique_agents,
            unique_ips_24h: unique_ips,
            top_agents,
            top_endpoints,
            generated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Get the current hour window string (e.g. "2026-03-25T14:00:00Z")
    fn current_hour_window() -> String {
        let now = chrono::Utc::now();
        format!("{}-{:02}-{:02}T{:02}:00:00Z",
            now.format("%Y"),
            now.format("%m"),
            now.format("%d"),
            now.format("%H"),
        )
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_record_serialization() {
        let record = UsageRecord {
            id: "rec-1".into(),
            agent_id: Some("agent-1".into()),
            ip_address: Some("192.168.1.1".into()),
            endpoint: "/api/registry".into(),
            method: "GET".into(),
            window_start: "2026-01-01T14:00:00Z".into(),
            request_count: 42,
            last_request: Some("2026-01-01T14:30:00Z".into()),
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["request_count"], 42);
        assert_eq!(json["endpoint"], "/api/registry");
    }

    #[test]
    fn test_agent_usage_summary_serialization() {
        let summary = AgentUsageSummary {
            agent_id: "agent-1".into(),
            total_requests_24h: 150,
            total_requests_7d: 1200,
            top_endpoints: vec![
                EndpointUsage {
                    endpoint: "/api/registry".into(),
                    method: "GET".into(),
                    request_count: 80,
                },
            ],
            generated_at: "2026-01-01T12:00:00Z".into(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["total_requests_24h"], 150);
        assert_eq!(json["top_endpoints"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_platform_usage_serialization() {
        let usage = PlatformUsage {
            total_requests_24h: 5000,
            total_requests_7d: 35000,
            unique_agents_24h: 42,
            unique_ips_24h: 128,
            top_agents: vec![
                AgentRequestCount { agent_id: "agent-1".into(), request_count: 500 },
            ],
            top_endpoints: vec![
                EndpointUsage {
                    endpoint: "/api/registry".into(),
                    method: "GET".into(),
                    request_count: 2000,
                },
            ],
            generated_at: "2026-01-01T12:00:00Z".into(),
        };
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["total_requests_24h"], 5000);
        assert_eq!(json["unique_agents_24h"], 42);
    }

    #[test]
    fn test_hour_window_format() {
        let window = RateLimitTracker::current_hour_window();
        assert!(window.ends_with(":00:00Z"));
        assert!(window.contains("T"));
        assert_eq!(window.len(), 20);
    }

    #[test]
    fn test_endpoint_usage_serialization() {
        let usage = EndpointUsage {
            endpoint: "/api/a2a/discover".into(),
            method: "GET".into(),
            request_count: 99,
        };
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["request_count"], 99);
        assert_eq!(json["method"], "GET");
    }
}
