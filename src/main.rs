#![allow(dead_code)]

mod scanner;
mod inferrer;
mod generator;
mod validator;
mod models;
mod verifier;
mod zk;
mod errors;
mod identity;
mod mcp;
mod openclaw;
mod registry;
mod ipfs;
mod eas;
mod a2a;
mod health;
mod analytics;
mod webhooks;
mod tags;
mod status;
mod export;
mod reviews;

mod x402;
mod agentic_wallet;

mod tests;
mod db;

mod farcaster;

use anyhow::{Result as AnyResult, Context};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json,
};
use rust_mcp_sdk::{
    mcp_server::{hyper_server, HyperServerOptions, ToMcpServerHandler},
    schema::*,
};
use clap::{Parser, Subcommand};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::SystemTime};
use std::result::Result as StdResult;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct AppState {
    redis_client: Arc<redis::Client>,
}

const RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
const RATE_LIMIT_MAX_REQUESTS: usize = 20;

fn random_emoji() -> &'static str {
    ["⬛", "⬜"].choose(&mut rand::thread_rng()).unwrap_or(&"⬛")
}

async fn check_rate_limit(state: &AppState, ip: &str) -> StdResult<(), StatusCode> {
    let key = format!("ratelimit:{}", ip);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut conn = state.redis_client
        .get_multiplexed_async_connection().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let results: StdResult<Vec<redis::Value>, _> = redis::pipe()
        .atomic()
        .zrembyscore(&key, 0, (now - RATE_LIMIT_WINDOW_SECONDS) as f64)
        .zadd(&key, now, now)
        .zcard(&key)
        .expire(&key, RATE_LIMIT_WINDOW_SECONDS as i64)
        .query_async(&mut conn)
        .await;

    let results = results.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let count: usize = if results.len() >= 3 {
        match &results[2] {
            redis::Value::Int(c) => *c as usize,
            _ => 0,
        }
    } else { 0 };

    if count > RATE_LIMIT_MAX_REQUESTS {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(())
}

#[derive(Parser)]
#[command(name = "beacon")]
#[command(about = "⬛ Make any repo agent-ready. Instantly.")]
#[command(version = VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate {
        target: String,
        #[arg(short, long, default_value = "AGENTS.md")]
        output: String,
        #[arg(long, default_value = "gemini")]
        provider: String,
        #[arg(long)]
        api_key: Option<String>,
    },
    Validate {
        file: String,
        #[arg(long)]
        check_endpoints: bool,
        #[arg(long)]
        provider: Option<String>,
    },
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    Register {
        #[arg(default_value = "./")]
        repo_path: String,
        #[arg(long, default_value = "base")]
        chain: String,
        #[arg(long)]
        agency: Option<String>,
    },
    Upgrade,
    FarcasterBot {
        #[arg(long, default_value = "30")]
        poll_interval: u64,
        #[arg(long, default_value = "beacon-agents")]
        channel: String,
        #[arg(long, default_value = "gemini")]
        provider: String,
    },
}

#[derive(Deserialize)]
struct GenerateRequest {
    #[serde(flatten)]
    repo_context: models::RepoContext,
    provider: Option<String>,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct ScanGenerateRequest {
    github_url: String,
    provider: Option<String>,
}

#[derive(Serialize)]
struct ScanGenerateResponse {
    manifest: models::AgentsManifest,
    agents_md: String,
}

#[derive(Serialize)]
struct GenerateResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    agents_md: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest: Option<models::AgentsManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Deserialize)]
struct ValidateRequest {
    content: String,
    provider: Option<String>,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct ValidateResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint_results: Option<Vec<models::EndpointCheckResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    name: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
        name: "beacon",
    })
}

async fn handle_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<GenerateRequest>,
) -> StdResult<impl IntoResponse, errors::BeaconError> {
    let ip = headers.get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    if let Err(status) = check_rate_limit(&state, &ip).await {
        return Ok(status.into_response());
    }

    let provider = req.provider.clone().unwrap_or_else(|| "gemini".to_string());
    let mut actual_provider = provider.clone();
    let mut rid_final = None;

    let is_cloud_request = req.api_key.is_none();

    if is_cloud_request {
        let txn_hash = headers.get("x-payment-txn-hash").and_then(|h| h.to_str().ok());
        let chain = headers.get("x-payment-chain").and_then(|h| h.to_str().ok());
        let run_id = headers.get("x-payment-run-id").and_then(|h| h.to_str().ok());

        if let (Some(txn), Some(ch), Some(rid)) = (txn_hash, chain, run_id) {
            rid_final = Some(rid.to_string());
            if db::payment_already_used(txn).await.unwrap_or(false) {
                return Err(errors::BeaconError::TransactionAlreadyUsed);
            }

            let amount = std::env::var("PAYMENT_AMOUNT_USDC")
                .unwrap_or_else(|_| "0.09".to_string())
                .parse::<f64>()
                .unwrap_or(0.09);
            let wallet = if ch == "base" {
                std::env::var("BEACON_WALLET_BASE").unwrap_or_default()
            } else {
                std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default()
            };

            let verified = verifier::verify_payment(ch, txn, amount, &wallet)
                .await
                .map_err(|e| errors::BeaconError::InferenceError(format!("Verification failed: {}", e)))?;

            if !verified {
                return Err(errors::BeaconError::CloudError { 
                    status: StatusCode::PAYMENT_REQUIRED.as_u16(), 
                    message: "Payment not verified".to_string() 
                });
            }

            db::mark_run_paid(rid, txn, ch).await.ok();
            db::record_payment(rid, txn, ch, None).await.ok();
            actual_provider = "gemini".to_string();
        } else {
            let rid = db::create_run(&req.repo_context.name)
                .await
                .map_err(|e| errors::BeaconError::DatabaseError(e.to_string()))?;

            let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
            let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
            let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();

            return Err(errors::BeaconError::PaymentRequired {
                run_id: rid,
                amount,
                base_addr: w_base,
                sol_addr: w_sol,
            });
        }
    }

    let manifest = inferrer::infer_capabilities(&req.repo_context, &actual_provider, req.api_key.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Inference failed: {}", e);
            errors::BeaconError::InferenceError(e.to_string())
        })?;

    let tmp_path = format!("/tmp/beacon_{}.md", &req.repo_context.name);
    generator::generate_agents_md(&manifest, &tmp_path)
        .map_err(|e| {
            tracing::error!("File generation failed: {}", e);
            errors::BeaconError::Unknown(format!("File generation failed: {}", e))
        })?;
    let content = std::fs::read_to_string(&tmp_path)
        .map_err(|e| {
            tracing::error!("Read generated file failed: {}", e);
            errors::BeaconError::IoError(e)
        })?;
    let _ = std::fs::remove_file(&tmp_path);

    if is_cloud_request {
        if let Some(rid) = rid_final {
            db::mark_run_complete(&rid, &content).await.ok();
        }
    }

    Ok(Json(GenerateResponse {
        success: true,
        agents_md: Some(content),
        manifest: Some(manifest),
        error: None,
    }).into_response())
}

async fn handle_validate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ValidateRequest>,
) -> StdResult<impl IntoResponse, errors::BeaconError> {
    let ip = headers.get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    if let Err(status) = check_rate_limit(&state, &ip).await {
        return Ok(status.into_response());
    }

    let is_cloud_request = req.api_key.is_none();

    if is_cloud_request {
        let txn_hash = headers.get("x-payment-txn-hash").and_then(|h| h.to_str().ok());
        let chain = headers.get("x-payment-chain").and_then(|h| h.to_str().ok());
        let run_id = headers.get("x-payment-run-id").and_then(|h| h.to_str().ok());

        if let (Some(txn), Some(ch), Some(rid)) = (txn_hash, chain, run_id) {
            if db::payment_already_used(txn).await.unwrap_or(false) {
                return Err(errors::BeaconError::TransactionAlreadyUsed);
            }

            let amount = std::env::var("PAYMENT_AMOUNT_USDC")
                .unwrap_or_else(|_| "0.09".to_string())
                .parse::<f64>()
                .unwrap_or(0.09);
            let wallet = if ch == "base" {
                std::env::var("BEACON_WALLET_BASE").unwrap_or_default()
            } else {
                std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default()
            };

            let verified = verifier::verify_payment(ch, txn, amount, &wallet)
                .await
                .map_err(|e| errors::BeaconError::ValidationError(format!("Verification failed: {}", e)))?;

            if !verified {
                return Err(errors::BeaconError::CloudError { 
                    status: StatusCode::PAYMENT_REQUIRED.as_u16(), 
                    message: "Payment not verified".to_string() 
                });
            }

            db::mark_run_paid(rid, txn, ch).await.ok();
            db::record_payment(rid, txn, ch, None).await.ok();
        } else {
            let rid = db::create_run("validate-only")
                .await
                .map_err(|e| errors::BeaconError::DatabaseError(e.to_string()))?;

            let amount = std::env::var("PAYMENT_AMOUNT_USDC").unwrap_or_else(|_| "0.09".to_string());
            let w_base = std::env::var("BEACON_WALLET_BASE").unwrap_or_default();
            let w_sol = std::env::var("BEACON_WALLET_SOLANA").unwrap_or_default();

            return Err(errors::BeaconError::PaymentRequired {
                run_id: rid,
                amount,
                base_addr: w_base,
                sol_addr: w_sol,
            });
        }
    }

    let result = validator::validate_content(&req.content)
        .map_err(|e| {
            tracing::error!("Validation failed: {}", e);
            errors::BeaconError::ValidationError(e.to_string())
        })?;

    Ok(Json(ValidateResponse {
        success: true,
        valid: Some(result.valid),
        errors: Some(result.errors),
        warnings: Some(result.warnings),
        endpoint_results: Some(result.endpoint_results),
        error: None,
    }).into_response())
}

// ── Scan & Generate (GitHub URL) ─────────────────────────────────────

async fn handle_scan_generate(
    Json(body): Json<ScanGenerateRequest>,
) -> StdResult<impl IntoResponse, (StatusCode, String)> {
    let provider = body.provider.as_deref().unwrap_or("gemini");
    let github_token = std::env::var("GITHUB_TOKEN").ok();

    let ctx = farcaster::github_scanner::scan_remote(&body.github_url, github_token.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("GitHub scan failed: {}", e);
            (StatusCode::BAD_REQUEST, format!("Scan failed: {}", e))
        })?;

    let manifest = inferrer::infer_capabilities(&ctx, provider, None)
        .await
        .map_err(|e| {
            tracing::error!("Inference failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Inference failed: {}", e))
        })?;

    let agents_md = generator::render_markdown(&manifest);

    Ok(Json(ScanGenerateResponse { manifest, agents_md }))
}

// ── Registry API Handlers ────────────────────────────────────────────

async fn handle_registry_search(
    Query(params): Query<registry::RegistryQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let reg = registry::AgentRegistry::new();
    match reg.search(&params).await {
        Ok(entries) => Ok(Json(entries).into_response()),
        Err(e) => {
            tracing::error!("Registry search failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_registry_register(
    Json(req): Json<registry::RegisterRequest>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let reg = registry::AgentRegistry::new();
    match reg.register(&req).await {
        Ok(resp) => Ok(Json(resp).into_response()),
        Err(e) => {
            tracing::error!("Registry register failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_registry_get(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let reg = registry::AgentRegistry::new();
    match reg.get_agent(&id).await {
        Ok(Some(entry)) => Ok(Json(entry).into_response()),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Registry get failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_basename_resolve(
    Path(name): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let reg = registry::AgentRegistry::new();
    match reg.resolve_basename(&name).await {
        Ok(Some(addr)) => Ok(Json(serde_json::json!({ "name": name, "address": addr })).into_response()),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Basename resolve failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── IPFS Pinning Handlers ────────────────────────────────────────────

async fn handle_registry_pin(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let agent = db::get_registry_agent(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let manifest = agent.manifest_json.clone();

    let client = ipfs::IpfsClient::from_env()
        .map_err(|e| {
            tracing::error!("IPFS client error: {}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let result = client.pin_json(&agent.name, &manifest).await
        .map_err(|e| {
            tracing::error!("IPFS pin failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Update the registry entry with the CID
    db::update_agent_manifest_cid(&id, &result.cid).await
        .map_err(|e| {
            tracing::error!("Failed to update CID: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result).into_response())
}

// ── EAS Attestation Handlers ────────────────────────────────────────

async fn handle_create_attestation(
    Path(id): Path<String>,
    Json(req): Json<eas::AttestRequest>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let agent = db::get_registry_agent(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let client = eas::EasClient::from_env()
        .map_err(|e| {
            tracing::error!("EAS client error: {}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let result = client.create_attestation(&req).await
        .map_err(|e| {
            tracing::error!("EAS attestation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Store attestation in DB
    db::insert_attestation(
        &id,
        &result.attestation_uid,
        &result.tx_hash,
        &result.schema_uid,
        agent.owner_address.as_deref().unwrap_or_default(),
    ).await.ok();

    Ok(Json(result).into_response())
}

async fn handle_get_attestation(
    Path(uid): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let attestation = db::get_attestation_by_uid(&uid).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(attestation).into_response())
}

async fn handle_get_agent_attestations(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let attestations = db::get_attestations_for_agent(&id).await
        .map_err(|e| {
            tracing::error!("Failed to get attestations: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(attestations).into_response())
}

// ── A2A Protocol Handlers ───────────────────────────────────────────

async fn handle_a2a_discover(
    Query(params): Query<a2a::DiscoveryQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match a2a::A2AProtocol::discover(&params).await {
        Ok(result) => Ok(Json(result).into_response()),
        Err(e) => {
            tracing::error!("A2A discovery failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_a2a_send(
    Json(msg): Json<a2a::A2AMessage>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match a2a::A2AProtocol::send_message(&msg).await {
        Ok(resp) => Ok(Json(resp).into_response()),
        Err(e) => {
            tracing::error!("A2A send failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_a2a_inbox(
    Path(agent_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let limit = params.get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(50usize)
        .min(100);

    match a2a::A2AProtocol::get_messages(&agent_id, limit).await {
        Ok(messages) => Ok(Json(messages).into_response()),
        Err(e) => {
            tracing::error!("A2A inbox failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_a2a_register_endpoint(
    Json(reg): Json<a2a::EndpointRegistration>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match a2a::A2AProtocol::register_endpoint(&reg).await {
        Ok(()) => Ok(Json(serde_json::json!({ "status": "ok" })).into_response()),
        Err(e) => {
            tracing::error!("A2A endpoint registration failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── Health Monitoring Handlers ──────────────────────────────────────

async fn handle_health_check_agent(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match health::HealthMonitor::check_agent(&id).await {
        Ok(status) => Ok(Json(status).into_response()),
        Err(e) => {
            tracing::error!("Health check failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_health_status(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match health::HealthMonitor::get_status(&id).await {
        Ok(Some(status)) => Ok(Json(status).into_response()),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Get health status failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_health_list(
    Query(params): Query<health::HealthQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let limit = 50;
    match health::HealthMonitor::list_statuses(params.status.as_deref(), limit).await {
        Ok(statuses) => Ok(Json(statuses).into_response()),
        Err(e) => {
            tracing::error!("List health statuses failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── Analytics Handlers ──────────────────────────────────────────────

async fn handle_agent_stats(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match analytics::AgentAnalytics::get_stats(&id).await {
        Ok(stats) => Ok(Json(stats).into_response()),
        Err(e) => {
            tracing::error!("Get agent stats failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_agent_events(
    Path(id): Path<String>,
    Query(params): Query<analytics::AnalyticsQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);
    match analytics::AgentAnalytics::get_events(&id, params.event_type.as_deref(), limit, offset).await {
        Ok(events) => Ok(Json(events).into_response()),
        Err(e) => {
            tracing::error!("Get agent events failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_trending_agents(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let limit = params.get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(20usize)
        .min(100);
    match analytics::AgentAnalytics::get_trending(limit).await {
        Ok(trending) => Ok(Json(trending).into_response()),
        Err(e) => {
            tracing::error!("Get trending agents failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── Webhook Handlers ────────────────────────────────────────────────

async fn handle_webhook_subscribe(
    Json(req): Json<webhooks::SubscribeRequest>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match webhooks::WebhookManager::subscribe(&req).await {
        Ok(sub) => Ok(Json(sub).into_response()),
        Err(e) => {
            tracing::error!("Webhook subscribe failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

async fn handle_webhook_unsubscribe(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match webhooks::WebhookManager::unsubscribe(&id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "status": "ok" })).into_response()),
        Err(e) => {
            tracing::error!("Webhook unsubscribe failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_webhook_list(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match webhooks::WebhookManager::get_subscriptions(&id).await {
        Ok(subs) => Ok(Json(subs).into_response()),
        Err(e) => {
            tracing::error!("Get webhooks failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── Tags & Categories Handlers ──────────────────────────────────────

async fn handle_set_tags(
    Path(id): Path<String>,
    Json(req): Json<tags::TagUpdateRequest>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match tags::AgentTags::set_tags(&id, &req.tags).await {
        Ok(tags) => Ok(Json(tags).into_response()),
        Err(e) => {
            tracing::error!("Set tags failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

async fn handle_get_tags(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match tags::AgentTags::get_tags(&id).await {
        Ok(tags) => Ok(Json(tags).into_response()),
        Err(e) => {
            tracing::error!("Get tags failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_search_by_tag(
    Query(params): Query<tags::TagQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let tag = params.tag.as_deref().unwrap_or("");
    if tag.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);
    match tags::AgentTags::search_by_tag(tag, limit, offset).await {
        Ok(agent_ids) => Ok(Json(agent_ids).into_response()),
        Err(e) => {
            tracing::error!("Search by tag failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_popular_tags(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let limit = params.get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(20usize)
        .min(100);
    match tags::AgentTags::get_popular_tags(limit).await {
        Ok(tags) => Ok(Json(tags).into_response()),
        Err(e) => {
            tracing::error!("Get popular tags failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_categories() -> impl IntoResponse {
    Json(tags::AgentTags::get_categories())
}

// ── x402 Payment Protocol Handlers ──────────────────────────────────

#[derive(Deserialize)]
struct X402PriceQuery {
    resource: Option<String>,
}

/// Returns x402 payment requirements for a given resource
async fn handle_x402_requirements(
    Query(params): Query<X402PriceQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let config = x402::X402Config::from_env();
    let resource = params.resource.as_deref().unwrap_or("/generate");
    let amount = std::env::var("PAYMENT_AMOUNT_USDC")
        .unwrap_or_else(|_| "0.09".to_string());
    let atomic = x402::usdc_to_atomic(&amount)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let requirements = x402::build_payment_requirements(resource, &atomic, &config);
    Ok(Json(requirements).into_response())
}

/// Verify an x402 payment signature
async fn handle_x402_verify(
    Json(body): Json<serde_json::Value>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let config = x402::X402Config::from_env();

    let payment_payload: x402::PaymentPayload = serde_json::from_value(
        body.get("paymentPayload").cloned().unwrap_or_default()
    ).map_err(|_| StatusCode::BAD_REQUEST)?;

    let payment_requirements: x402::PaymentRequirements = serde_json::from_value(
        body.get("paymentRequirements").cloned().unwrap_or_default()
    ).map_err(|_| StatusCode::BAD_REQUEST)?;

    match x402::verify_payment(&config.facilitator_url, &payment_payload, &payment_requirements).await {
        Ok(valid) => Ok(Json(serde_json::json!({ "success": valid })).into_response()),
        Err(e) => {
            tracing::error!("x402 verify failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Settle an x402 payment
async fn handle_x402_settle(
    Json(body): Json<serde_json::Value>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let config = x402::X402Config::from_env();

    let payment_payload: x402::PaymentPayload = serde_json::from_value(
        body.get("paymentPayload").cloned().unwrap_or_default()
    ).map_err(|_| StatusCode::BAD_REQUEST)?;

    let payment_requirements: x402::PaymentRequirements = serde_json::from_value(
        body.get("paymentRequirements").cloned().unwrap_or_default()
    ).map_err(|_| StatusCode::BAD_REQUEST)?;

    match x402::settle_payment(&config.facilitator_url, &payment_payload, &payment_requirements).await {
        Ok(result) => Ok(Json(result).into_response()),
        Err(e) => {
            tracing::error!("x402 settle failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Discover x402-enabled agents/endpoints
async fn handle_x402_discover() -> StdResult<impl IntoResponse, StatusCode> {
    // Search registry for agents that have x402-enabled endpoints
    let entries = db::search_registry(None, 100, 0).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let x402_agents: Vec<serde_json::Value> = entries.into_iter().filter_map(|e| {
        let endpoints = e.manifest_json.get("endpoints")?.as_array()?;
        let x402_endpoints: Vec<&serde_json::Value> = endpoints.iter()
            .filter(|ep| ep.get("x402_enabled").and_then(|v| v.as_bool()).unwrap_or(false))
            .collect();

        if x402_endpoints.is_empty() {
            return None;
        }

        Some(serde_json::json!({
            "agent_id": e.id.to_string(),
            "name": e.name,
            "description": e.description,
            "x402_endpoints": x402_endpoints,
        }))
    }).collect();

    Ok(Json(x402_agents).into_response())
}

// ── Agentic Wallet Handlers ─────────────────────────────────────────

/// Get wallet info for an agent
async fn handle_get_wallet(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let agent = db::get_registry_agent(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    match &agent.wallet_address {
        Some(addr) => Ok(Json(serde_json::json!({
            "agent_id": id,
            "wallet_address": addr,
            "chain": "base",
        })).into_response()),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Provision or link a wallet to an agent
async fn handle_provision_wallet(
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> StdResult<impl IntoResponse, StatusCode> {
    // Check agent exists
    let _agent = db::get_registry_agent(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Try to provision via CDP, or accept a manual wallet address
    let wallet_address = if let Some(addr) = body.get("wallet_address").and_then(|v| v.as_str()) {
        addr.to_string()
    } else {
        // Auto-provision via Coinbase CDP
        match agentic_wallet::provision_wallet(&id).await {
            Ok(wallet) => wallet.wallet_address,
            Err(e) => {
                tracing::error!("Wallet provisioning failed: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    };

    // Update in database
    db::update_agent_wallet(&id, &wallet_address).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "agent_id": id,
        "wallet_address": wallet_address,
        "chain": "base",
    })).into_response())
}

// ── Status Page Handlers ────────────────────────────────────────────

async fn handle_registry_status() -> StdResult<impl IntoResponse, StatusCode> {
    match status::StatusPage::get_status().await {
        Ok(status) => Ok(Json(status).into_response()),
        Err(e) => {
            tracing::error!("Get registry status failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── Export Agent Card Handlers ──────────────────────────────────────

async fn handle_export_card(
    Path(id): Path<String>,
    Query(params): Query<export::ExportQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let format = params.format.as_deref().unwrap_or("json-ld");
    match format {
        "a2a" => match export::AgentExport::export_a2a(&id).await {
            Ok(card) => Ok(Json(card).into_response()),
            Err(e) => {
                tracing::error!("Export A2A card failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
        _ => match export::AgentExport::export_card(&id).await {
            Ok(card) => Ok(Json(card).into_response()),
            Err(e) => {
                tracing::error!("Export agent card failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
    }
}

// ── Reviews & Ratings Handlers ──────────────────────────────────────

async fn handle_create_review(
    Path(id): Path<String>,
    Json(req): Json<reviews::CreateReviewRequest>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match reviews::AgentReviews::create_review(&id, &req).await {
        Ok(review) => Ok(Json(review).into_response()),
        Err(e) => {
            tracing::error!("Create review failed: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

async fn handle_get_reviews(
    Path(id): Path<String>,
    Query(params): Query<reviews::ReviewQuery>,
) -> StdResult<impl IntoResponse, StatusCode> {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);
    match reviews::AgentReviews::get_reviews(&id, limit, offset).await {
        Ok(reviews) => Ok(Json(reviews).into_response()),
        Err(e) => {
            tracing::error!("Get reviews failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_rating_summary(
    Path(id): Path<String>,
) -> StdResult<impl IntoResponse, StatusCode> {
    match reviews::AgentReviews::get_summary(&id).await {
        Ok(summary) => Ok(Json(summary).into_response()),
        Err(e) => {
            tracing::error!("Get rating summary failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn handle_top_rated() -> StdResult<impl IntoResponse, StatusCode> {
    match reviews::AgentReviews::get_top_rated(20).await {
        Ok(top) => Ok(Json(top).into_response()),
        Err(e) => {
            tracing::error!("Get top rated failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Generate {
            target,
            output,
            provider,
            api_key,
        } => {
            println!("{} Beacon — scanning {}...", random_emoji(), target);
            let ctx = scanner::scan_local(&target)?;
            println!("📦 Repo: {} ({} source files)", ctx.name, ctx.source_files.len());
            let manifest = inferrer::infer_capabilities(&ctx, &provider, api_key.as_deref()).await?;
            generator::generate_agents_md(&manifest, &output)?;
            println!("\n✅ Done! AGENTS.md written to: {}", output);
            println!("   Provider:     {}", provider);
            println!("   Capabilities: {}", manifest.capabilities.len());
            println!("   Endpoints:    {}", manifest.endpoints.len());
        }
        Commands::Validate {
            file,
            check_endpoints,
            provider,
        } => {
            println!("{} Beacon — validating {}...", random_emoji(), file);
            let content =
                std::fs::read_to_string(&file).map_err(|_| anyhow::anyhow!("File not found: {}", file))?;

            let mut result = if let Some(p) = provider {
                if p == "beacon-ai-cloud" {
                    validator::validate_cloud(&content).await?
                } else {
                    validator::validate_content(&content)?
                }
            } else {
                validator::validate_content(&content)?
            };

            if check_endpoints {
                println!("   🌐 Checking endpoint reachability...");
                result.endpoint_results = validator::check_endpoints(&content).await?;
            }
            println!("\n📋 Validation Report");
            println!("   Valid:    {}", if result.valid { "✅ Yes" } else { "❌ No" });
            println!("   Errors:   {}", result.errors.len());
            println!("   Warnings: {}", result.warnings.len());
            if !result.errors.is_empty() {
                println!("\n❌ Errors:");
                for e in &result.errors {
                    println!("   • {}", e);
                }
            }
            if !result.warnings.is_empty() {
                println!("\n⚠️  Warnings:");
                for w in &result.warnings {
                    println!("   • {}", w);
                }
            }
            if !result.endpoint_results.is_empty() {
                println!("\n🌐 Endpoint Results:");
                for ep in &result.endpoint_results {
                    let status = ep.status_code.map(|s| s.to_string()).unwrap_or_else(|| "—".to_string());
                    println!(
                        "   {} {} ({})",
                        if ep.reachable { "✅" } else { "❌" },
                        ep.endpoint,
                        status
                    );
                }
            }
        }
        Commands::Serve { port } => {
            let redis_url = std::env::var("REDIS_URL").context("REDIS_URL must be set")?;
            let state = AppState {
                redis_client: Arc::new(redis::Client::open(redis_url)?),
            };

            let server_info = InitializeResult {
                server_info: Implementation {
                    name: "beacon-mcp".into(),
                    version: VERSION.into(),
                    title: Some("Beacon MCP Server".into()),
                    description: Some("Make any repo agent-ready. Instantly.".into()),
                    icons: vec![],
                    website_url: Some("https://beaconcloud.org".into()),
                },
                capabilities: ServerCapabilities {
                    tools: Some(ServerCapabilitiesTools { list_changed: Some(false) }),
                    ..Default::default()
                },
                instructions: None,
                meta: None,
                protocol_version: "2025-11-25".into(),
            };

            let mcp_handler = mcp::BeaconMcpHandler::default();

            let server = hyper_server::create_server(
                server_info,
                mcp_handler.to_mcp_server_handler(),
                HyperServerOptions {
                    host: "0.0.0.0".to_string(),
                    port,
                    sse_support: true,
                    ..Default::default()
                },
            );

            let server = server
                .with_route("/health", get(health))
                .with_route("/validate", post(handle_validate).with_state(state.clone()))
                .with_route("/generate", post(handle_generate).with_state(state))
                .with_route("/api/generate", post(handle_scan_generate))
                .with_route("/api/registry", get(handle_registry_search))
                .with_route("/api/registry", post(handle_registry_register))
                .with_route("/api/registry/{id}", get(handle_registry_get))
                .with_route("/api/basenames/resolve/{name}", get(handle_basename_resolve))
                // IPFS pinning
                .with_route("/api/registry/{id}/pin", post(handle_registry_pin))
                // EAS attestations
                .with_route("/api/registry/{id}/attest", post(handle_create_attestation))
                .with_route("/api/registry/{id}/attestations", get(handle_get_agent_attestations))
                .with_route("/api/attestations/{uid}", get(handle_get_attestation))
                // A2A protocol
                .with_route("/api/a2a/discover", get(handle_a2a_discover))
                .with_route("/api/a2a/messages", post(handle_a2a_send))
                .with_route("/api/a2a/messages/{agent_id}", get(handle_a2a_inbox))
                .with_route("/api/a2a/endpoint", post(handle_a2a_register_endpoint))
                // Health monitoring
                .with_route("/api/registry/{id}/health", post(handle_health_check_agent))
                .with_route("/api/registry/{id}/health", get(handle_health_status))
                .with_route("/api/health", get(handle_health_list))
                // Analytics
                .with_route("/api/registry/{id}/stats", get(handle_agent_stats))
                .with_route("/api/registry/{id}/events", get(handle_agent_events))
                .with_route("/api/analytics/trending", get(handle_trending_agents))
                // Tags & categories
                .with_route("/api/registry/{id}/tags", put(handle_set_tags))
                .with_route("/api/registry/{id}/tags", get(handle_get_tags))
                .with_route("/api/tags/search", get(handle_search_by_tag))
                .with_route("/api/tags/popular", get(handle_popular_tags))
                .with_route("/api/tags/categories", get(handle_categories))
                // x402 payment protocol
                .with_route("/api/x402/requirements", get(handle_x402_requirements))
                .with_route("/api/x402/verify", post(handle_x402_verify))
                .with_route("/api/x402/settle", post(handle_x402_settle))
                .with_route("/api/x402/discover", get(handle_x402_discover))
                // Agentic wallets
                .with_route("/api/registry/{id}/wallet", get(handle_get_wallet))
                .with_route("/api/registry/{id}/wallet", post(handle_provision_wallet))
                // Status page
                .with_route("/api/status", get(handle_registry_status))
                // Export agent card
                .with_route("/api/registry/{id}/export", get(handle_export_card))
                // Reviews & ratings
                .with_route("/api/registry/{id}/reviews", post(handle_create_review))
                .with_route("/api/registry/{id}/reviews", get(handle_get_reviews))
                .with_route("/api/registry/{id}/rating", get(handle_rating_summary))
                .with_route("/api/reviews/top", get(handle_top_rated))
                .with_route("/.well-known/farcaster.json", get(farcaster::miniapp::handle_farcaster_manifest))
                .with_route("/miniapp", get(farcaster::miniapp::handle_miniapp_home))
                .with_route("/miniapp/agent/{id}", get(farcaster::miniapp::handle_miniapp_agent))
                .with_route("/og/agent/{id}", get(farcaster::og::handle_og_image))
                .with_route("/og/default.png", get(farcaster::og::handle_og_default));

            println!("{} Beacon API & MCP Server", random_emoji());
            println!("   http://0.0.0.0:{}", port);
            println!("   POST /generate                      — generate AGENTS.md");
            println!("   POST /validate                      — validate AGENTS.md");
            println!("   GET  /api/registry                  — search agent registry");
            println!("   POST /api/registry                  — register an agent");
            println!("   GET  /api/registry/{{id}}              — get agent by ID");
            println!("   POST /api/registry/{{id}}/pin         — pin manifest to IPFS");
            println!("   POST /api/registry/{{id}}/attest      — create EAS attestation");
            println!("   GET  /api/registry/{{id}}/attestations — get agent attestations");
            println!("   GET  /api/attestations/{{uid}}         — get attestation by UID");
            println!("   GET  /api/basenames/{{name}}           — resolve basename");
            println!("   GET  /api/a2a/discover              — discover agents by capability");
            println!("   POST /api/a2a/messages              — send agent-to-agent message");
            println!("   GET  /api/a2a/messages/{{id}}          — get agent inbox");
            println!("   POST /api/a2a/endpoint              — register agent endpoint");
            println!("   POST /api/registry/{{id}}/health      — ping agent health check");
            println!("   GET  /api/registry/{{id}}/health      — get agent health status");
            println!("   GET  /api/health                    — list all health statuses");
            println!("   GET  /api/registry/{{id}}/stats       — get agent analytics");
            println!("   GET  /api/registry/{{id}}/events      — get agent event log");
            println!("   GET  /api/analytics/trending        — trending agents");
            println!("   PUT  /api/registry/{{id}}/tags        — set agent tags");
            println!("   GET  /api/registry/{{id}}/tags        — get agent tags");
            println!("   GET  /api/tags/search               — search agents by tag");
            println!("   GET  /api/tags/popular              — popular tags");
            println!("   GET  /api/tags/categories           — list categories");
            println!("   GET  /api/x402/requirements         — get x402 payment requirements");
            println!("   POST /api/x402/verify               — verify x402 payment");
            println!("   POST /api/x402/settle               — settle x402 payment");
            println!("   GET  /api/x402/discover             — discover x402-enabled agents");
            println!("   GET  /api/registry/{{id}}/wallet      — get agent wallet");
            println!("   POST /api/registry/{{id}}/wallet      — provision agent wallet");
            println!("   GET  /api/status                    — registry status page");
            println!("   GET  /api/registry/{{id}}/export      — export agent card");
            println!("   POST /api/registry/{{id}}/reviews     — submit a review");
            println!("   GET  /api/registry/{{id}}/reviews     — get agent reviews");
            println!("   GET  /api/registry/{{id}}/rating      — get rating summary");
            println!("   GET  /api/reviews/top               — top rated agents");
            println!("   GET  /sse                           — MCP Server (SSE)");
            println!("   GET  /health                        — health check");

            // Start farcaster bot in background if configured
            if std::env::var("NEYNAR_API_KEY").is_ok() {
                match farcaster::neynar::NeynarClient::from_env() {
                    Ok(neynar) => {
                        let bot_config = farcaster::bot::BotConfig::new(
                            std::env::var("FARCASTER_CHANNEL").unwrap_or_else(|_| "beacon-agents".to_string()),
                            30,
                            "gemini".to_string(),
                        );
                        
                        let bot_pool = db::DbPool::new().expect("Failed to create db pool");
                        tokio::spawn(async move {
                            if let Err(e) = farcaster::bot::run_bot(Arc::new(neynar), bot_config, bot_pool).await {
                                tracing::error!("Farcaster bot error: {}", e);
                            }
                        });
                        println!("   🤖 Farcaster bot starting...");

                        // Start auto-poster in background (posts every 30 min)
                        let autoposter_neynar = Arc::new(
                            farcaster::neynar::NeynarClient::from_env()
                                .expect("Neynar client already validated")
                        );
                        let autoposter_config = farcaster::autoposter::AutoPosterConfig::new(
                            std::env::var("AUTOPOSTER_INTERVAL_SECS")
                                .ok()
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(1800), // 30 minutes default
                        );
                        tokio::spawn(async move {
                            if let Err(e) = farcaster::autoposter::run_autoposter(
                                autoposter_neynar,
                                autoposter_config,
                            ).await {
                                tracing::error!("Auto-poster error: {}", e);
                            }
                        });
                        println!("   📣 Auto-poster starting (every {}s)...",
                            std::env::var("AUTOPOSTER_INTERVAL_SECS")
                                .unwrap_or_else(|_| "1800".to_string()));
                    }
                    Err(e) => {
                        tracing::warn!("Farcaster bot not starting: {}", e);
                    }
                }
            }

            server.start().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        Commands::Register { repo_path, chain, agency } => {
            println!("{} Registering on-chain agent identity...", random_emoji());
            let _chain = chain;
            identity::register_agent_identity(&repo_path, &_chain, agency.as_deref()).await?;
            println!("\n✅ Done! Agent identity registered.");
        }
        Commands::Upgrade => {
            println!("{} Upgrading Beacon CLI...", random_emoji());
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg("curl -fsSL https://raw.githubusercontent.com/BeaconOnBase/beacon/master/install.sh | sh")
                .status()?;

            if !status.success() {
                anyhow::bail!("Upgrade failed with status: {}", status);
            }
        }
        Commands::FarcasterBot { poll_interval, channel, provider } => {
            println!("{} Starting Farcaster Bot...", random_emoji());
            
            let neynar = farcaster::neynar::NeynarClient::from_env()
                .context("Failed to initialize Neynar client")?;
            
            let db_pool = db::DbPool::new()
                .context("Failed to create database pool")?;

            let bot_config = farcaster::bot::BotConfig::new(
                channel,
                poll_interval,
                provider,
            );

            farcaster::bot::run_bot(Arc::new(neynar), bot_config, db_pool).await?;
        }
    }
    Ok(())
}
