#![allow(dead_code)]

mod scanner;
mod inferrer;
mod generator;
mod validator;
mod models;
mod verifier;
mod errors;
mod identity;
mod mcp;
mod openclaw;
mod registry;
mod ipfs;
mod eas;
mod a2a;
mod tags;

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

    let manifest = agent.manifest_json
        .ok_or(StatusCode::BAD_REQUEST)?;

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
        &agent.owner_address,
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
;

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
