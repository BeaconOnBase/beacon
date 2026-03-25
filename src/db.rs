use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use crate::models::AgentsManifest;

// ── Agent Registry (Supabase/PostgREST) ─────────────────────────────

const AGENT_REGISTRY_TABLE: &str = "agent_manifests";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentRegistryEntry {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub manifest_json: serde_json::Value,
    #[serde(default)]
    pub capabilities_count: i32,
    #[serde(default)]
    pub endpoints_count: i32,
    pub run_id: Option<String>,
    pub on_chain_id: Option<String>,
    pub fid: Option<i64>,
    pub created_at: Option<String>,
    // Fields used by registry but not in DB — kept for API compat
    #[serde(default)]
    pub basename: Option<String>,
    #[serde(default)]
    pub manifest_cid: Option<String>,
    #[serde(default)]
    pub owner_address: Option<String>,
    #[serde(default)]
    pub wallet_address: Option<String>,
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub tx_hash: Option<String>,
}

pub async fn register_agent(entry: &AgentRegistryEntry) -> Result<()> {
    let db = client()?;

    db.from(AGENT_REGISTRY_TABLE)
        .insert(json!([{
            "name": entry.name,
            "description": entry.description,
            "manifest_json": entry.manifest_json,
            "capabilities_count": entry.capabilities_count,
            "endpoints_count": entry.endpoints_count,
            "run_id": entry.run_id,
            "on_chain_id": entry.on_chain_id,
            "fid": entry.fid,
        }]).to_string())
        .execute()
        .await
        .context("Failed to register agent")?;

    Ok(())
}

pub async fn search_registry(query: Option<&str>, limit: usize, offset: usize) -> Result<Vec<AgentRegistryEntry>> {
    let db = client()?;

    let resp = if let Some(q) = query {
        if q.is_empty() {
            db.from(AGENT_REGISTRY_TABLE)
                .select("*")
                .order("created_at.desc")
                .range(offset, offset + limit - 1)
                .execute()
                .await
                .context("Failed to search registry")?
        } else {
            db.from(AGENT_REGISTRY_TABLE)
                .select("*")
                .or(format!("name.ilike.%{}%,description.ilike.%{}%", q, q))
                .order("created_at.desc")
                .range(offset, offset + limit - 1)
                .execute()
                .await
                .context("Failed to search registry")?
        }
    } else {
        db.from(AGENT_REGISTRY_TABLE)
            .select("*")
            .order("created_at.desc")
            .range(offset, offset + limit - 1)
            .execute()
            .await
            .context("Failed to search registry")?
    };

    let body = resp.text().await?;
    let entries: Vec<AgentRegistryEntry> = serde_json::from_str(&body)?;
    Ok(entries)
}

pub async fn get_registry_agent(id: &str) -> Result<Option<AgentRegistryEntry>> {
    let db = client()?;

    let resp = db.from(AGENT_REGISTRY_TABLE)
        .eq("id", id)
        .select("*")
        .execute()
        .await
        .context("Failed to get registry agent")?;

    let body = resp.text().await?;
    let entries: Vec<AgentRegistryEntry> = serde_json::from_str(&body)?;
    Ok(entries.into_iter().next())
}

pub async fn update_agent_manifest_cid(agent_id: &str, cid: &str) -> Result<()> {
    let db = client()?;

    db.from(AGENT_REGISTRY_TABLE)
        .eq("id", agent_id)
        .update(json!({
            "manifest_cid": cid
        }).to_string())
        .execute()
        .await
        .context("Failed to update manifest CID")?;

    Ok(())
}

pub async fn search_registry_advanced(
    capability: Option<&str>,
    framework: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<AgentRegistryEntry>> {
    let db = client()?;

    let mut query = db.from(AGENT_REGISTRY_TABLE)
        .select("*")
        .order("created_at.desc")
        .range(offset, offset + limit - 1);

    if let Some(fw) = framework {
        query = query.eq("framework", fw);
    }

    if let Some(cap) = capability {
        query = query.ilike("manifest_json", format!("%{}%", cap));
    }

    let resp = query.execute().await
        .context("Failed to search registry (advanced)")?;

    let body = resp.text().await?;
    let entries: Vec<AgentRegistryEntry> = serde_json::from_str(&body)?;
    Ok(entries)
}

// ── EAS Attestations ────────────────────────────────────────────────

const ATTESTATIONS_TABLE: &str = "agent_attestations";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentAttestationRow {
    pub id: String,
    pub agent_id: String,
    pub attestation_uid: String,
    pub schema_uid: String,
    pub tx_hash: String,
    pub attester: String,
    pub revoked: Option<bool>,
    pub created_at: Option<String>,
}

pub async fn insert_attestation(
    agent_id: &str,
    attestation_uid: &str,
    tx_hash: &str,
    schema_uid: &str,
    attester: &str,
) -> Result<()> {
    let db = client()?;

    db.from(ATTESTATIONS_TABLE)
        .insert(json!([{
            "id": uuid::Uuid::new_v4().to_string(),
            "agent_id": agent_id,
            "attestation_uid": attestation_uid,
            "schema_uid": schema_uid,
            "tx_hash": tx_hash,
            "attester": attester,
            "revoked": false,
        }]).to_string())
        .execute()
        .await
        .context("Failed to insert attestation")?;

    Ok(())
}

pub async fn get_attestations_for_agent(agent_id: &str) -> Result<Vec<AgentAttestationRow>> {
    let db = client()?;

    let resp = db.from(ATTESTATIONS_TABLE)
        .eq("agent_id", agent_id)
        .select("*")
        .order("created_at.desc")
        .execute()
        .await
        .context("Failed to get attestations")?;

    let body = resp.text().await?;
    let rows: Vec<AgentAttestationRow> = serde_json::from_str(&body)?;
    Ok(rows)
}

pub async fn get_attestation_by_uid(uid: &str) -> Result<Option<AgentAttestationRow>> {
    let db = client()?;

    let resp = db.from(ATTESTATIONS_TABLE)
        .eq("attestation_uid", uid)
        .select("*")
        .execute()
        .await
        .context("Failed to get attestation")?;

    let body = resp.text().await?;
    let rows: Vec<AgentAttestationRow> = serde_json::from_str(&body)?;
    Ok(rows.into_iter().next())
}

// ── A2A Messaging ───────────────────────────────────────────────────

const A2A_MESSAGES_TABLE: &str = "a2a_messages";
const A2A_ENDPOINTS_TABLE: &str = "a2a_endpoints";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct A2AMessageRow {
    pub id: String,
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub reply_to: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
}

pub async fn insert_a2a_message(msg: &A2AMessageRow) -> Result<()> {
    let db = client()?;

    db.from(A2A_MESSAGES_TABLE)
        .insert(json!([{
            "id": msg.id,
            "from_agent_id": msg.from_agent_id,
            "to_agent_id": msg.to_agent_id,
            "message_type": msg.message_type,
            "payload": msg.payload,
            "reply_to": msg.reply_to,
            "status": msg.status,
        }]).to_string())
        .execute()
        .await
        .context("Failed to insert A2A message")?;

    Ok(())
}

pub async fn get_a2a_messages(agent_id: &str, limit: usize) -> Result<Vec<A2AMessageRow>> {
    let db = client()?;

    let resp = db.from(A2A_MESSAGES_TABLE)
        .eq("to_agent_id", agent_id)
        .select("*")
        .order("created_at.desc")
        .range(0, limit - 1)
        .execute()
        .await
        .context("Failed to get A2A messages")?;

    let body = resp.text().await?;
    let rows: Vec<A2AMessageRow> = serde_json::from_str(&body)?;
    Ok(rows)
}

pub async fn upsert_a2a_endpoint(agent_id: &str, endpoint_url: &str) -> Result<()> {
    let db = client()?;

    db.from(A2A_ENDPOINTS_TABLE)
        .upsert(json!([{
            "agent_id": agent_id,
            "endpoint_url": endpoint_url,
            "updated_at": chrono::Utc::now().to_rfc3339(),
        }]).to_string())
        .execute()
        .await
        .context("Failed to upsert A2A endpoint")?;

    Ok(())
}

pub async fn get_a2a_endpoint(agent_id: &str) -> Result<Option<String>> {
    let db = client()?;

    let resp = db.from(A2A_ENDPOINTS_TABLE)
        .eq("agent_id", agent_id)
        .select("endpoint_url")
        .execute()
        .await
        .context("Failed to get A2A endpoint")?;

    let body = resp.text().await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body)?;
    Ok(rows.first().and_then(|r| r["endpoint_url"].as_str().map(|s| s.to_string())))
}

// ── Health Monitoring ────────────────────────────────────────────────

const HEALTH_STATUS_TABLE: &str = "agent_health_status";

pub async fn upsert_health_status(status: &crate::health::HealthStatus) -> Result<()> {
    let db = client()?;

    db.from(HEALTH_STATUS_TABLE)
        .upsert(json!([{
            "agent_id": status.agent_id,
            "status": status.status.to_string(),
            "latency_ms": status.latency_ms,
            "last_checked": status.last_checked,
            "endpoint": status.endpoint,
            "error": status.error,
        }]).to_string())
        .execute()
        .await
        .context("Failed to upsert health status")?;

    Ok(())
}

pub async fn get_health_status(agent_id: &str) -> Result<Option<crate::health::HealthStatus>> {
    let db = client()?;

    let resp = db.from(HEALTH_STATUS_TABLE)
        .eq("agent_id", agent_id)
        .select("*")
        .execute()
        .await
        .context("Failed to get health status")?;

    let body = resp.text().await?;
    let rows: Vec<crate::health::HealthStatus> = serde_json::from_str(&body)?;
    Ok(rows.into_iter().next())
}

pub async fn list_health_statuses(
    status_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<crate::health::HealthStatus>> {
    let db = client()?;

    let mut query = db.from(HEALTH_STATUS_TABLE)
        .select("*")
        .order("last_checked.desc")
        .range(0, limit - 1);

    if let Some(status) = status_filter {
        query = query.eq("status", status);
    }

    let resp = query.execute().await
        .context("Failed to list health statuses")?;

    let body = resp.text().await?;
    let rows: Vec<crate::health::HealthStatus> = serde_json::from_str(&body)?;
    Ok(rows)
}

// ── Agent Analytics ─────────────────────────────────────────────────

const ANALYTICS_EVENTS_TABLE: &str = "analytics_events";
const AGENT_STATS_TABLE: &str = "agent_stats";

pub async fn insert_analytics_event(event: &crate::analytics::AnalyticsEvent) -> Result<()> {
    let db = client()?;

    db.from(ANALYTICS_EVENTS_TABLE)
        .insert(json!([{
            "id": event.id,
            "agent_id": event.agent_id,
            "event_type": event.event_type,
            "metadata": event.metadata,
        }]).to_string())
        .execute()
        .await
        .context("Failed to insert analytics event")?;

    Ok(())
}

pub async fn get_analytics_events(
    agent_id: &str,
    event_type: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<crate::analytics::AnalyticsEvent>> {
    let db = client()?;

    let mut query = db.from(ANALYTICS_EVENTS_TABLE)
        .eq("agent_id", agent_id)
        .select("*")
        .order("created_at.desc")
        .range(offset, offset + limit - 1);

    if let Some(et) = event_type {
        query = query.eq("event_type", et);
    }

    let resp = query.execute().await
        .context("Failed to get analytics events")?;

    let body = resp.text().await?;
    let rows: Vec<crate::analytics::AnalyticsEvent> = serde_json::from_str(&body)?;
    Ok(rows)
}

pub async fn get_agent_stats(agent_id: &str) -> Result<crate::analytics::AgentStats> {
    let db = client()?;

    let resp = db.from(AGENT_STATS_TABLE)
        .eq("agent_id", agent_id)
        .select("*")
        .execute()
        .await
        .context("Failed to get agent stats")?;

    let body = resp.text().await?;
    let rows: Vec<crate::analytics::AgentStats> = serde_json::from_str(&body)?;

    Ok(rows.into_iter().next().unwrap_or(crate::analytics::AgentStats {
        agent_id: agent_id.to_string(),
        total_discoveries: 0,
        total_messages_received: 0,
        total_messages_sent: 0,
        total_attestations: 0,
        total_health_checks: 0,
        last_active: None,
    }))
}

pub async fn get_trending_agents(limit: usize) -> Result<Vec<crate::analytics::AgentStats>> {
    let db = client()?;

    let resp = db.from(AGENT_STATS_TABLE)
        .select("*")
        .order("total_discoveries.desc")
        .range(0, limit - 1)
        .execute()
        .await
        .context("Failed to get trending agents")?;

    let body = resp.text().await?;
    let rows: Vec<crate::analytics::AgentStats> = serde_json::from_str(&body)?;
    Ok(rows)
}

// ── Agent Tags ──────────────────────────────────────────────────────

const AGENT_TAGS_TABLE: &str = "agent_tags";

pub async fn replace_agent_tags(agent_id: &str, tags: &[String]) -> Result<()> {
    let db = client()?;

    // Delete existing tags
    db.from(AGENT_TAGS_TABLE)
        .eq("agent_id", agent_id)
        .delete()
        .execute()
        .await
        .context("Failed to delete existing tags")?;

    // Insert new tags
    if !tags.is_empty() {
        let rows: Vec<serde_json::Value> = tags.iter().map(|tag| {
            json!({
                "id": Uuid::new_v4().to_string(),
                "agent_id": agent_id,
                "tag": tag,
            })
        }).collect();

        db.from(AGENT_TAGS_TABLE)
            .insert(serde_json::to_string(&rows)?)
            .execute()
            .await
            .context("Failed to insert tags")?;
    }

    Ok(())
}

pub async fn get_agent_tags(agent_id: &str) -> Result<Vec<crate::tags::AgentTag>> {
    let db = client()?;

    let resp = db.from(AGENT_TAGS_TABLE)
        .eq("agent_id", agent_id)
        .select("*")
        .order("tag.asc")
        .execute()
        .await
        .context("Failed to get agent tags")?;

    let body = resp.text().await?;
    let rows: Vec<crate::tags::AgentTag> = serde_json::from_str(&body)?;
    Ok(rows)
}

pub async fn delete_agent_tag(agent_id: &str, tag: &str) -> Result<()> {
    let db = client()?;

    db.from(AGENT_TAGS_TABLE)
        .eq("agent_id", agent_id)
        .eq("tag", tag)
        .delete()
        .execute()
        .await
        .context("Failed to delete agent tag")?;

    Ok(())
}

pub async fn get_agents_by_tag(tag: &str, limit: usize, offset: usize) -> Result<Vec<String>> {
    let db = client()?;

    let resp = db.from(AGENT_TAGS_TABLE)
        .eq("tag", tag)
        .select("agent_id")
        .range(offset, offset + limit - 1)
        .execute()
        .await
        .context("Failed to get agents by tag")?;

    let body = resp.text().await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body)?;
    Ok(rows.iter().filter_map(|r| r["agent_id"].as_str().map(|s| s.to_string())).collect())
}

pub async fn get_popular_tags(limit: usize) -> Result<Vec<crate::tags::TagCount>> {
    let db = client()?;

    // Get all tags and count client-side (PostgREST doesn't support GROUP BY easily)
    let resp = db.from(AGENT_TAGS_TABLE)
        .select("tag")
        .execute()
        .await
        .context("Failed to get tags for counting")?;

    let body = resp.text().await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body)?;

    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for row in &rows {
        if let Some(tag) = row["tag"].as_str() {
            *counts.entry(tag.to_string()).or_insert(0) += 1;
        }
    }

    let mut tag_counts: Vec<crate::tags::TagCount> = counts.into_iter()
        .map(|(tag, count)| crate::tags::TagCount { tag, count })
        .collect();

    tag_counts.sort_by(|a, b| b.count.cmp(&a.count));
    tag_counts.truncate(limit);

    Ok(tag_counts)
}

// ── Status Page ─────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct RegistryCounts {
    pub total: i64,
    pub attestations: i64,
    pub messages: i64,
}

#[derive(Debug, Default)]
pub struct HealthCounts {
    pub online: i64,
    pub offline: i64,
    pub degraded: i64,
}

pub async fn get_registry_counts() -> Result<RegistryCounts> {
    let db = client()?;

    let resp = db.from(AGENT_REGISTRY_TABLE)
        .select("id")
        .execute()
        .await
        .context("Failed to count agents")?;

    let body = resp.text().await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body)?;
    let total = rows.len() as i64;

    let attestations = match db.from(ATTESTATIONS_TABLE)
        .select("id")
        .execute()
        .await
    {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_else(|_| "[]".into());
            let rows: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap_or_default();
            rows.len() as i64
        }
        Err(_) => 0,
    };

    let messages = match db.from(A2A_MESSAGES_TABLE)
        .select("id")
        .execute()
        .await
    {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_else(|_| "[]".into());
            let rows: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap_or_default();
            rows.len() as i64
        }
        Err(_) => 0,
    };

    Ok(RegistryCounts {
        total,
        attestations,
        messages,
    })
}

pub async fn get_health_counts() -> Result<HealthCounts> {
    let db = client()?;

    let resp = db.from(HEALTH_STATUS_TABLE)
        .select("status")
        .execute()
        .await
        .context("Failed to get health counts")?;

    let body = resp.text().await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body)?;

    let mut counts = HealthCounts::default();
    for row in &rows {
        match row["status"].as_str() {
            Some("online") => counts.online += 1,
            Some("offline") => counts.offline += 1,
            Some("degraded") => counts.degraded += 1,
            _ => {}
        }
    }

    Ok(counts)
}

pub async fn get_top_tag_names(limit: usize) -> Result<Vec<String>> {
    let tags = get_popular_tags(limit).await?;
    Ok(tags.into_iter().map(|t| t.tag).collect())
}

pub async fn get_recent_agents(limit: usize) -> Result<Vec<crate::status::RecentAgent>> {
    let db = client()?;

    let resp = db.from(AGENT_REGISTRY_TABLE)
        .select("id,name,description,registered_at")
        .order("registered_at.desc")
        .range(0, limit - 1)
        .execute()
        .await
        .context("Failed to get recent agents")?;

    let body = resp.text().await?;
    let rows: Vec<crate::status::RecentAgent> = serde_json::from_str(&body)?;
    Ok(rows)
}

// ── PostgREST / Supabase (Cloud API) ────────────────────────────────


const RUNS_TABLE: &str = "runs";
const PAYMENTS_TABLE: &str = "payments";
const AGENT_MANIFESTS_TABLE: &str = "agent_manifests";
const FARCASTER_SCANS_TABLE: &str = "farcaster_scans";

#[derive(Clone)]
pub struct DbPool {
    client: postgrest::Postgrest,
}

impl DbPool {
    pub fn new() -> Result<Self> {
        let url = std::env::var("SUPABASE_URL")
            .context("SUPABASE_URL not set")?;
        let key = std::env::var("SUPABASE_SERVICE_KEY")
            .context("SUPABASE_SERVICE_KEY not set")?;

        let client = postgrest::Postgrest::new(format!("{}/rest/v1", url))
            .insert_header("apikey", &key)
            .insert_header("Authorization", format!("Bearer {}", key));

        Ok(Self { client })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentManifestRow {
    pub id: Uuid,
    pub run_id: Option<String>,
    pub name: String,
    pub description: String,
    pub manifest_json: serde_json::Value,
    pub capabilities_count: i32,
    pub endpoints_count: i32,
    pub on_chain_id: Option<String>,
    pub fid: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub repo_name: String,
    pub provider: String,
    pub status: String,
    pub txn_hash: Option<String>,
    pub chain: Option<String>,
    pub agents_md: Option<String>,
    pub error: Option<String>,
}



#[derive(Debug, Serialize, Deserialize)]
pub struct Payment {
    pub id: String,
    pub run_id: String,
    pub txn_hash: String,
    pub chain: String,
    pub amount_usdc: f64,
    pub from_address: Option<String>,
    pub confirmed: bool,
}

fn client() -> Result<postgrest::Postgrest> {
    let url = std::env::var("SUPABASE_URL")
        .context("SUPABASE_URL not set")?;
    let key = std::env::var("SUPABASE_SERVICE_KEY")
        .context("SUPABASE_SERVICE_KEY not set")?;

    Ok(postgrest::Postgrest::new(format!("{}/rest/v1", url))
        .insert_header("apikey", &key)
        .insert_header("Authorization", format!("Bearer {}", key)))
}




pub async fn create_run(repo_name: &str) -> Result<String> {
    let db = client()?;
    let run_id = uuid::Uuid::new_v4().to_string();

    db.from(RUNS_TABLE)
        .insert(json!([{
            "id": run_id,
            "repo_name": repo_name,
            "provider": "beacon-ai-cloud",
            "status": "pending"
        }]).to_string())
        .execute()
        .await
        .context("Failed to create run")?;

    Ok(run_id)
}

pub async fn mark_run_paid(run_id: &str, txn_hash: &str, chain: &str) -> Result<()> {
    let db = client()?;

    db.from(RUNS_TABLE)
        .eq("id", run_id)
        .update(json!({
            "status": "paid",
            "txn_hash": txn_hash,
            "chain": chain
        }).to_string())
        .execute()
        .await
        .context("Failed to mark run as paid")?;

    Ok(())
}



pub async fn mark_run_complete(run_id: &str, agents_md: &str) -> Result<()> {

    let db = client()?;

    db.from(RUNS_TABLE)
        .eq("id", run_id)
        .update(json!({
            "status": "complete",
            "agents_md": agents_md
        }).to_string())
        .execute()
        .await
        .context("Failed to mark run complete")?;

    Ok(())
}

pub async fn mark_run_failed(run_id: &str, error: &str) -> Result<()> {
    let db = client()?;

    db.from(RUNS_TABLE)
        .eq("id", run_id)
        .update(json!({
            "status": "failed",
            "error": error
        }).to_string())
        .execute()
        .await
        .context("Failed to mark run failed")?;

    Ok(())
}



pub async fn record_payment(
    run_id: &str,
    txn_hash: &str,
    chain: &str,
    from_address: Option<&str>,
) -> Result<()> {
    let db = client()?;
    let amount = std::env::var("PAYMENT_AMOUNT_USDC")
        .unwrap_or("0.09".to_string())
        .parse::<f64>()
        .unwrap_or(0.09);

    db.from(PAYMENTS_TABLE)
        .insert(json!([{
            "id": uuid::Uuid::new_v4().to_string(),
            "run_id": run_id,
            "txn_hash": txn_hash,
            "chain": chain,
            "amount_usdc": amount, "from_address": from_address,
            "confirmed": true, "confirmed_at": chrono::Utc::now().to_rfc3339()
        }]).to_string())
        .execute()
        .await
        .context("Failed to record payment")?;

    Ok(())
}

pub async fn payment_already_used(txn_hash: &str) -> Result<bool> {
    let db = client()?;

    let resp = db.from(PAYMENTS_TABLE)
        .eq("txn_hash", txn_hash)
        .select("id")
        .execute()
        .await
        .context("Failed to check payment")?;

    let body = resp.text().await?;
    let records: serde_json::Value = serde_json::from_str(&body)?;
    Ok(records.as_array().map(|a| !a.is_empty()).unwrap_or(false))
}

// Farcaster bot database functions

pub async fn scan_exists(pool: &DbPool, cast_hash: &str) -> Result<bool> {
    let resp = pool.client.from(FARCASTER_SCANS_TABLE)
        .eq("cast_hash", cast_hash)
        .select("id")
        .execute()
        .await
        .context("Failed to check scan")?;

    let body = resp.text().await?;
    let records: serde_json::Value = serde_json::from_str(&body)?;
    Ok(records.as_array().map(|a| !a.is_empty()).unwrap_or(false))
}

pub async fn insert_farcaster_scan(pool: &DbPool, cast_hash: &str, github_url: &str) -> Result<Uuid> {
    let id = Uuid::new_v4();
    
    pool.client.from(FARCASTER_SCANS_TABLE)
        .insert(json!([{
            "id": id.to_string(),
            "cast_hash": cast_hash,
            "github_url": github_url,
            "status": "pending"
        }]).to_string())
        .execute()
        .await
        .context("Failed to insert farcaster scan")?;

    Ok(id)
}

pub async fn update_farcaster_scan(
    pool: &DbPool,
    id: Uuid,
    status: &str,
    agents_md: Option<&str>,
    reply_hash: Option<&str>,
) -> Result<()> {
    let mut update_data = json!({
        "status": status
    });

    if let Some(md) = agents_md {
        update_data["agents_md"] = json!(md);
    }
    if let Some(reply) = reply_hash {
        update_data["reply_hash"] = json!(reply);
    }

    pool.client.from(FARCASTER_SCANS_TABLE)
        .eq("id", id.to_string())
        .update(update_data.to_string())
        .execute()
        .await
        .context("Failed to update farcaster scan")?;

    Ok(())
}

pub async fn insert_agent_manifest(
    pool: &DbPool,
    manifest: &AgentsManifest,
    on_chain_id: Option<&str>,
    fid: i64,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let manifest_json = serde_json::to_value(manifest)?;

    pool.client.from(AGENT_MANIFESTS_TABLE)
        .insert(json!([{
            "id": id.to_string(),
            "run_id": Option::<String>::None,
            "name": manifest.name,
            "description": manifest.description,
            "manifest_json": manifest_json,
            "capabilities_count": manifest.capabilities.len() as i32,
            "endpoints_count": manifest.endpoints.len() as i32,
            "on_chain_id": on_chain_id,
            "fid": fid,
            "created_at": chrono::Utc::now().to_rfc3339()
        }]).to_string())
        .execute()
        .await
        .context("Failed to insert agent manifest")?;

    Ok(id)
}

pub async fn get_agent(pool: &DbPool, id: Uuid) -> Result<Option<AgentManifestRow>> {
    let resp = pool.client.from(AGENT_MANIFESTS_TABLE)
        .eq("id", id.to_string())
        .select("*")
        .execute()
        .await
        .context("Failed to get agent")?;

    let body = resp.text().await?;
    let records: Vec<AgentManifestRow> = serde_json::from_str(&body)?;
    Ok(records.into_iter().next())
}

pub async fn search_agents(
    _pool: &DbPool,
    query: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AgentManifestRow>> {
    let mut url = format!("{}/{}?select=*&limit={}&offset={}", 
        std::env::var("SUPABASE_URL").context("SUPABASE_URL not set")? + "/rest/v1",
        AGENT_MANIFESTS_TABLE,
        limit,
        offset
    );

    if let Some(q) = query {
        url.push_str(&format!("&name=ilike.%{}%", q));
    }

    let key = std::env::var("SUPABASE_SERVICE_KEY")
        .context("SUPABASE_SERVICE_KEY not set")?;

    let resp = reqwest::Client::new()
        .get(&url)
        .header("apikey", &key)
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await
        .context("Failed to search agents")?;

    let records: Vec<AgentManifestRow> = resp.json().await.context("Failed to parse response")?;
    Ok(records)
}