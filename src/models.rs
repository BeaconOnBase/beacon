#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentsManifest {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub agent_identity: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    pub authentication: Option<Authentication>,
    pub rate_limits: Option<RateLimits>,
    pub contact: Option<String>,
    // ZK Fields
    pub source_hash: Option<String>,
    pub zk_proof: Option<String>,
    pub generation_timestamp: Option<i64>,
}

// ── Google A2A Agent Card ───────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub protocol_version: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub url: String,
    pub icon_url: Option<String>,
    pub provider: Option<AgentProvider>,
    pub capabilities: AgentCardCapabilities,
    pub skills: Vec<AgentSkill>,
    #[serde(default)]
    pub security_schemes: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentProvider {
    pub organization: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentCardCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub name: String,
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Capability {
    pub name: String,
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Endpoint {
    pub path: String,
    pub method: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    // x402 payment fields
    #[serde(default)]
    pub x402_enabled: bool,
    #[serde(default)]
    pub price_per_call: Option<String>,
    #[serde(default)]
    pub payment_currency: Option<String>,
    #[serde(default)]
    pub payment_network: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Parameter {
    pub name: String,
    pub r#type: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Authentication {
    pub r#type: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RateLimits {
    pub requests_per_minute: Option<u32>,
    pub requests_per_day: Option<u32>,
    pub notes: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RepoContext {
    pub name: String,
    pub readme: Option<String>,
    pub source_files: Vec<SourceFile>,
    pub openapi_spec: Option<String>,
    pub package_manifest: Option<String>,
    pub existing_agents_md: Option<String>,
    pub agent_framework: Option<AgentFramework>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AgentFramework {
    pub name: String,
    pub version: Option<String>,
    pub config_files: Vec<String>,
    pub detected_capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub language: Language,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Other(String),
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Language::Rust,
            "py" => Language::Python,
            "ts" => Language::TypeScript,
            "js" => Language::JavaScript,
            "go" => Language::Go,
            other => Language::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub endpoint_results: Vec<EndpointCheckResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EndpointCheckResult {
    pub endpoint: String,
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}
