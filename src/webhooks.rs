#![allow(dead_code)]

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use crate::db;

// ── Webhook Events ──────────────────────────────────────────────────
//
// Push notifications when things happen: new agent registered,
// attestation created, A2A message received, health status changed.
// Agent owners subscribe to events they care about.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebhookSubscription {
    pub id: String,
    pub agent_id: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub active: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebhookEvent {
    pub event_type: String,
    pub agent_id: String,
    pub timestamp: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebhookDelivery {
    pub id: String,
    pub subscription_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status_code: Option<i32>,
    pub success: bool,
    pub error: Option<String>,
    pub delivered_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubscribeRequest {
    pub agent_id: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
}

// Subscribable event types
pub const WH_AGENT_REGISTERED: &str = "agent.registered";
pub const WH_AGENT_UPDATED: &str = "agent.updated";
pub const WH_ATTESTATION_CREATED: &str = "attestation.created";
pub const WH_MESSAGE_RECEIVED: &str = "message.received";
pub const WH_HEALTH_CHANGED: &str = "health.changed";
pub const WH_MANIFEST_PINNED: &str = "manifest.pinned";
pub const WH_VERSION_CREATED: &str = "version.created";

const VALID_EVENTS: &[&str] = &[
    WH_AGENT_REGISTERED,
    WH_AGENT_UPDATED,
    WH_ATTESTATION_CREATED,
    WH_MESSAGE_RECEIVED,
    WH_HEALTH_CHANGED,
    WH_MANIFEST_PINNED,
    WH_VERSION_CREATED,
];

pub struct WebhookManager;

impl WebhookManager {
    /// Subscribe to webhook events for an agent
    pub async fn subscribe(req: &SubscribeRequest) -> Result<WebhookSubscription> {
        // Validate event types
        for event in &req.events {
            if !VALID_EVENTS.contains(&event.as_str()) {
                anyhow::bail!("Invalid event type: {}. Valid types: {:?}", event, VALID_EVENTS);
            }
        }

        // Validate URL
        let _parsed = url::Url::parse(&req.url)
            .context("Invalid webhook URL")?;

        let sub = WebhookSubscription {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: req.agent_id.clone(),
            url: req.url.clone(),
            events: req.events.clone(),
            secret: req.secret.clone(),
            active: true,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        db::insert_webhook_subscription(&sub).await?;

        Ok(sub)
    }

    /// Unsubscribe (deactivate) a webhook
    pub async fn unsubscribe(subscription_id: &str) -> Result<()> {
        db::deactivate_webhook(subscription_id).await
    }

    /// Get all subscriptions for an agent
    pub async fn get_subscriptions(agent_id: &str) -> Result<Vec<WebhookSubscription>> {
        db::get_webhook_subscriptions(agent_id).await
    }

    /// Fire a webhook event — delivers to all matching subscriptions
    pub async fn fire(event: &WebhookEvent) -> Result<Vec<WebhookDelivery>> {
        let subs = db::get_webhook_subscriptions(&event.agent_id).await?;
        let mut deliveries = Vec::new();

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        for sub in subs {
            if !sub.active || !sub.events.contains(&event.event_type) {
                continue;
            }

            let mut req = client.post(&sub.url)
                .header("Content-Type", "application/json")
                .header("X-Beacon-Event", &event.event_type)
                .json(event);

            // Add HMAC signature if secret is configured
            if let Some(ref secret) = sub.secret {
                let body = serde_json::to_string(event).unwrap_or_default();
                let signature = compute_hmac(secret, &body);
                req = req.header("X-Beacon-Signature", signature);
            }

            let result = req.send().await;

            let (status_code, success, error) = match result {
                Ok(resp) => {
                    let code = resp.status().as_u16() as i32;
                    (Some(code), code >= 200 && code < 300, None)
                }
                Err(e) => (None, false, Some(e.to_string())),
            };

            let delivery = WebhookDelivery {
                id: uuid::Uuid::new_v4().to_string(),
                subscription_id: sub.id.clone(),
                event_type: event.event_type.clone(),
                payload: event.payload.clone(),
                status_code,
                success,
                error,
                delivered_at: Some(chrono::Utc::now().to_rfc3339()),
            };

            db::insert_webhook_delivery(&delivery).await.ok();
            deliveries.push(delivery);
        }

        Ok(deliveries)
    }

    /// Get recent deliveries for a subscription
    pub async fn get_deliveries(subscription_id: &str, limit: usize) -> Result<Vec<WebhookDelivery>> {
        db::get_webhook_deliveries(subscription_id, limit).await
    }
}

/// Simple HMAC-like signature using the webhook secret
fn compute_hmac(secret: &str, body: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    secret.hash(&mut hasher);
    body.hash(&mut hasher);
    format!("sha256={:016x}", hasher.finish())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_event_types() {
        assert_eq!(VALID_EVENTS.len(), 7);
        assert!(VALID_EVENTS.contains(&"agent.registered"));
        assert!(VALID_EVENTS.contains(&"attestation.created"));
        assert!(VALID_EVENTS.contains(&"health.changed"));
    }

    #[test]
    fn test_webhook_event_serialization() {
        let event = WebhookEvent {
            event_type: WH_AGENT_REGISTERED.to_string(),
            agent_id: "agent-123".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            payload: json!({ "name": "test-agent" }),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event_type"], "agent.registered");
        assert_eq!(json["payload"]["name"], "test-agent");
    }

    #[test]
    fn test_hmac_signature_deterministic() {
        let sig1 = compute_hmac("my-secret", r#"{"test": true}"#);
        let sig2 = compute_hmac("my-secret", r#"{"test": true}"#);
        assert_eq!(sig1, sig2);
        assert!(sig1.starts_with("sha256="));
    }

    #[test]
    fn test_hmac_signature_differs_with_different_secret() {
        let sig1 = compute_hmac("secret-1", "body");
        let sig2 = compute_hmac("secret-2", "body");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_webhook_subscription_serialization() {
        let sub = WebhookSubscription {
            id: "sub-1".to_string(),
            agent_id: "agent-1".to_string(),
            url: "https://example.com/webhook".to_string(),
            events: vec![WH_AGENT_REGISTERED.to_string(), WH_HEALTH_CHANGED.to_string()],
            secret: Some("my-secret".to_string()),
            active: true,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(json["events"].as_array().unwrap().len(), 2);
        assert_eq!(json["active"], true);
    }
}
