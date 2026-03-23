use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use crate::models::AgentsManifest;

// ── Agent Registry (Supabase/PostgREST) ─────────────────────────────

const AGENT_REGISTRY_TABLE: &str = "agent_registry";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentRegistryEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub basename: Option<String>,
    pub manifest_json: Option<serde_json::Value>,
    pub manifest_cid: Option<String>,
    pub owner_address: String,
    pub wallet_address: Option<String>,
    pub framework: Option<String>,
    pub tx_hash: Option<String>,
    pub registered_at: Option<String>,
}

pub async fn register_agent(entry: &AgentRegistryEntry) -> Result<()> {
    let db = client()?;

    db.from(AGENT_REGISTRY_TABLE)
        .insert(json!([{
            "id": entry.id,
            "name": entry.name,
            "description": entry.description,
            "basename": entry.basename,
            "manifest_json": entry.manifest_json,
            "manifest_cid": entry.manifest_cid,
            "owner_address": entry.owner_address,
            "wallet_address": entry.wallet_address,
            "framework": entry.framework,
            "tx_hash": entry.tx_hash,
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
                .order("registered_at.desc")
                .range(offset, offset + limit - 1)
                .execute()
                .await
                .context("Failed to search registry")?
        } else {
            db.from(AGENT_REGISTRY_TABLE)
                .select("*")
                .or(format!("name.ilike.%{}%,description.ilike.%{}%", q, q))
                .order("registered_at.desc")
                .range(offset, offset + limit - 1)
                .execute()
                .await
                .context("Failed to search registry")?
        }
    } else {
        db.from(AGENT_REGISTRY_TABLE)
            .select("*")
            .order("registered_at.desc")
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
        .order("registered_at.desc")
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