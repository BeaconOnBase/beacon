use anyhow::{Context, Result};
use ethers_core::types::{Address, TransactionRequest, NameOrAddress};
use ethers_core::abi::{self, Token};
use ethers_core::utils::keccak256;
use ethers_providers::{Provider, Http, Middleware};
use serde::{Deserialize, Serialize};

/// Onchain Agent Registry — stores agent manifests on Base
/// Uses Supabase/PostgREST for storage, Base L2 for name resolution

// Registry entry as stored/returned from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub basename: Option<String>,
    pub manifest_cid: Option<String>,
    pub owner: String,
    pub wallet_address: Option<String>,
    pub registered_at: u64,
    pub tx_hash: Option<String>,
}

// Request to register a new agent
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub description: String,
    pub basename: Option<String>,
    pub manifest_json: serde_json::Value,
    pub owner_address: String,
}

// Response after registration
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub agent_id: String,
    pub tx_hash: Option<String>,
    pub registry_url: String,
}

// Registry query parameters
#[derive(Debug, Deserialize)]
pub struct RegistryQuery {
    pub query: Option<String>,
    pub owner: Option<String>,
    pub framework: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Agent Registry backed by Supabase/PostgREST + Base L2 name resolution
pub struct AgentRegistry {
    base_rpc_url: String,
}

impl AgentRegistry {
    pub fn new() -> Self {
        let base_rpc_url = std::env::var("BASE_RPC_URL")
            .unwrap_or_else(|_| "https://mainnet.base.org".to_string());
        Self { base_rpc_url }
    }

    /// Register an agent in the Supabase registry
    pub async fn register(
        &self,
        req: &RegisterRequest,
    ) -> Result<RegisterResponse> {
        let agent_id = uuid::Uuid::new_v4().to_string();
        let manifest_json = serde_json::to_value(&req.manifest_json)?;

        let caps_count = req.manifest_json.get("capabilities")
            .and_then(|c| c.as_array())
            .map(|a| a.len() as i32)
            .unwrap_or(0);
        let eps_count = req.manifest_json.get("endpoints")
            .and_then(|e| e.as_array())
            .map(|a| a.len() as i32)
            .unwrap_or(0);

        let entry = crate::db::AgentRegistryEntry {
            id: uuid::Uuid::parse_str(&agent_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            name: req.name.clone(),
            description: req.description.clone(),
            manifest_json: manifest_json,
            capabilities_count: caps_count,
            endpoints_count: eps_count,
            run_id: None,
            on_chain_id: None,
            fid: None,
            created_at: None,
            basename: req.basename.clone(),
            manifest_cid: None,
            owner_address: Some(req.owner_address.clone()),
            wallet_address: None,
            framework: None,
            tx_hash: None,
        };

        crate::db::register_agent(&entry).await?;

        Ok(RegisterResponse {
            agent_id: agent_id.clone(),
            tx_hash: None,
            registry_url: format!("/api/registry/{}", agent_id),
        })
    }

    /// Search the agent registry
    pub async fn search(
        &self,
        query: &RegistryQuery,
    ) -> Result<Vec<RegistryEntry>> {
        let limit = query.limit.unwrap_or(20).min(100);
        let offset = query.offset.unwrap_or(0);

        let entries = crate::db::search_registry(
            query.query.as_deref(),
            limit,
            offset,
        ).await?;

        Ok(entries.into_iter().map(|e| RegistryEntry {
            agent_id: e.id.to_string(),
            name: e.name,
            description: e.description,
            basename: e.basename,
            manifest_cid: e.manifest_cid,
            owner: e.owner_address.unwrap_or_default(),
            wallet_address: e.wallet_address,
            registered_at: 0,
            tx_hash: e.tx_hash,
        }).collect())
    }

    /// Get a single agent from the registry by ID
    pub async fn get_agent(
        &self,
        agent_id: &str,
    ) -> Result<Option<RegistryEntry>> {
        let entry = crate::db::get_registry_agent(agent_id).await?;

        Ok(entry.map(|e| RegistryEntry {
            agent_id: e.id.to_string(),
            name: e.name,
            description: e.description,
            basename: e.basename,
            manifest_cid: e.manifest_cid,
            owner: e.owner_address.unwrap_or_default(),
            wallet_address: e.wallet_address,
            registered_at: 0,
            tx_hash: e.tx_hash,
        }))
    }

    /// Resolve a basename to a wallet address using the Base L2 ENS resolver.
    ///
    /// Basenames use a custom L2Resolver on Base (not standard Ethereum ENS).
    /// - L2Resolver: 0xC6d566A56A1aFf6508b41f6c90ff131615583BCD
    /// - Registry:   0xb94704422c2a1e396835a571837aa5ae53285a95
    ///
    /// We call `addr(bytes32 node)` on the L2Resolver with the namehash of the name.
    pub async fn resolve_basename(&self, basename: &str) -> Result<Option<String>> {
        let provider = Provider::<Http>::try_from(self.base_rpc_url.as_str())
            .context("Failed to create Base provider")?;

        let l2_resolver: Address = "0xC6d566A56A1aFf6508b41f6c90ff131615583BCD"
            .parse()
            .context("Invalid L2Resolver address")?;

        // Compute ENS namehash for the basename
        let name = if basename.ends_with(".base.eth") {
            basename.to_string()
        } else if basename.ends_with(".eth") {
            basename.to_string()
        } else {
            format!("{}.base.eth", basename)
        };

        let node = namehash(&name);

        // Encode `addr(bytes32)` call — selector 0x3b3b57de
        let call_data = abi::encode(&[Token::FixedBytes(node.to_vec())]);
        let mut data = vec![0x3b, 0x3b, 0x57, 0xde]; // addr(bytes32) selector
        data.extend_from_slice(&call_data);

        let tx = TransactionRequest::new()
            .to(NameOrAddress::Address(l2_resolver))
            .data(data);

        match provider.call(&tx.into(), None).await {
            Ok(result) => {
                if result.len() >= 32 {
                    let addr_bytes = &result[12..32];
                    let address = Address::from_slice(addr_bytes);
                    if address.is_zero() {
                        Ok(None)
                    } else {
                        Ok(Some(format!("{:?}", address)))
                    }
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }
}

/// Public wrapper for testing
#[cfg(test)]
pub fn namehash_public(name: &str) -> [u8; 32] {
    namehash(name)
}

/// Compute ENS namehash per EIP-137.
/// namehash('') = 0x0000...0000
/// namehash('eth') = keccak256(namehash('') + keccak256('eth'))
/// namehash('base.eth') = keccak256(namehash('eth') + keccak256('base'))
fn namehash(name: &str) -> [u8; 32] {
    let mut node = [0u8; 32];

    if name.is_empty() {
        return node;
    }

    let labels: Vec<&str> = name.split('.').collect();
    for label in labels.iter().rev() {
        let label_hash = keccak256(label.as_bytes());
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(&node);
        combined.extend_from_slice(&label_hash);
        node = keccak256(&combined);
    }

    node
}
