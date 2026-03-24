use anyhow::{Context, Result};
use ethers_core::abi::{self, Token};
use ethers_core::types::{Address, Bytes, TransactionRequest, NameOrAddress, U256};
use ethers_core::utils::keccak256;
use ethers_providers::{Provider, Http, Middleware};
use ethers_signers::{LocalWallet, Signer};
use ethers_middleware::SignerMiddleware;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// EAS (Ethereum Attestation Service) client for Base mainnet.
/// Creates onchain attestations that verify agent capabilities, ownership, and audit status.
///
/// Base Mainnet EAS contract: 0x4200000000000000000000000000000000000021
/// Base Mainnet SchemaRegistry: 0x4200000000000000000000000000000000000020

const EAS_CONTRACT: &str = "0x4200000000000000000000000000000000000021";

/// Schema: bytes32 agentId, string name, address owner, string manifestCid, string capabilities, bool audited
/// This schema encodes agent identity and verification data.
const SCHEMA_STRING: &str = "bytes32 agentId,string name,address owner,string manifestCid,string capabilities,bool audited";

pub struct EasClient {
    provider_url: String,
    schema_uid: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AttestationResult {
    pub attestation_uid: String,
    pub tx_hash: String,
    pub schema_uid: String,
    pub eas_url: String,
}

#[derive(Debug, Deserialize)]
pub struct AttestRequest {
    pub agent_id: String,
    pub name: String,
    pub owner_address: String,
    pub manifest_cid: Option<String>,
    pub capabilities: Vec<String>,
    pub audited: bool,
}

impl EasClient {
    /// Create EAS client from environment variables.
    /// Requires: BASE_RPC_URL (optional, defaults to mainnet), EAS_SCHEMA_UID
    pub fn from_env() -> Result<Self> {
        let provider_url = std::env::var("BASE_RPC_URL")
            .unwrap_or_else(|_| "https://mainnet.base.org".to_string());

        let schema_uid_hex = std::env::var("EAS_SCHEMA_UID")
            .context("EAS_SCHEMA_UID not set")?;

        let schema_uid_hex = schema_uid_hex.strip_prefix("0x").unwrap_or(&schema_uid_hex);
        let schema_uid_bytes = hex::decode(schema_uid_hex)
            .context("Invalid EAS_SCHEMA_UID hex")?;

        let mut schema_uid = [0u8; 32];
        if schema_uid_bytes.len() != 32 {
            anyhow::bail!("EAS_SCHEMA_UID must be 32 bytes");
        }
        schema_uid.copy_from_slice(&schema_uid_bytes);

        Ok(Self {
            provider_url,
            schema_uid,
        })
    }

    /// Compute the schema UID for our attestation schema.
    /// schema_uid = keccak256(abi.encodePacked(schema, resolver_address, revocable))
    pub fn compute_schema_uid() -> [u8; 32] {
        let mut data = Vec::new();
        data.extend_from_slice(SCHEMA_STRING.as_bytes());
        data.extend_from_slice(&[0u8; 20]); // zero resolver address
        data.push(1u8); // revocable = true
        keccak256(&data)
    }

    /// Create an onchain attestation for an agent.
    pub async fn create_attestation(&self, req: &AttestRequest) -> Result<AttestationResult> {
        let provider = Provider::<Http>::try_from(self.provider_url.as_str())
            .context("Failed to create Base provider")?;

        let private_key = std::env::var("AGENT_PRIVATE_KEY")
            .context("AGENT_PRIVATE_KEY not set for EAS attestation signing")?;

        let wallet: LocalWallet = private_key.parse::<LocalWallet>()
            .context("Invalid AGENT_PRIVATE_KEY")?
            .with_chain_id(8453u64); // Base mainnet

        let client = Arc::new(SignerMiddleware::new(provider, wallet));

        let eas_address: Address = EAS_CONTRACT.parse()
            .context("Invalid EAS contract address")?;

        let owner_address: Address = req.owner_address.parse()
            .context("Invalid owner address")?;

        // Encode the agent ID as bytes32
        let mut agent_id_bytes = [0u8; 32];
        let id_hash = keccak256(req.agent_id.as_bytes());
        agent_id_bytes.copy_from_slice(&id_hash);

        // Encode capabilities as comma-separated string
        let capabilities_str = req.capabilities.join(",");

        // ABI-encode the attestation data matching our schema
        let encoded_data = abi::encode(&[
            Token::FixedBytes(agent_id_bytes.to_vec()),
            Token::String(req.name.clone()),
            Token::Address(owner_address),
            Token::String(req.manifest_cid.clone().unwrap_or_default()),
            Token::String(capabilities_str),
            Token::Bool(req.audited),
        ]);

        // Build the AttestationRequestData struct:
        // struct AttestationRequestData {
        //     address recipient;
        //     uint64 expirationTime;
        //     bool revocable;
        //     bytes32 refUID;
        //     bytes data;
        //     uint256 value;
        // }
        let attestation_data = abi::encode(&[
            Token::Address(owner_address),     // recipient
            Token::Uint(U256::zero()),         // expirationTime (0 = no expiration)
            Token::Bool(true),                 // revocable
            Token::FixedBytes(vec![0u8; 32]),  // refUID (no reference)
            Token::Bytes(encoded_data),        // data
            Token::Uint(U256::zero()),         // value (no ETH)
        ]);

        // Build the AttestationRequest:
        // struct AttestationRequest {
        //     bytes32 schema;
        //     AttestationRequestData data;
        // }
        let full_request = abi::encode(&[
            Token::FixedBytes(self.schema_uid.to_vec()),
            Token::Bytes(attestation_data),
        ]);

        // attest((bytes32,(...))) selector
        let selector = &keccak256(b"attest((bytes32,(address,uint64,bool,bytes32,bytes,uint256)))")[..4];
        let mut calldata = selector.to_vec();
        calldata.extend_from_slice(&full_request);

        let tx = TransactionRequest::new()
            .to(NameOrAddress::Address(eas_address))
            .data(Bytes::from(calldata));

        let pending_tx = client.send_transaction(tx, None).await
            .context("Failed to send EAS attestation transaction")?;

        let tx_hash = format!("{:?}", pending_tx.tx_hash());

        // Wait for confirmation
        let receipt = pending_tx.await
            .context("Failed to confirm EAS attestation")?
            .context("Transaction receipt not found")?;

        // The attestation UID is returned in the first log's data
        let attestation_uid = if let Some(log) = receipt.logs.first() {
            if log.data.len() >= 32 {
                format!("0x{}", hex::encode(&log.data[..32]))
            } else {
                format!("0x{}", hex::encode(&receipt.transaction_hash.as_bytes()))
            }
        } else {
            format!("0x{}", hex::encode(&receipt.transaction_hash.as_bytes()))
        };

        let schema_uid_hex = format!("0x{}", hex::encode(self.schema_uid));

        let eas_url = format!("https://base.easscan.org/attestation/view/{}", attestation_uid);

        Ok(AttestationResult {
            attestation_uid,
            tx_hash,
            schema_uid: schema_uid_hex,
            eas_url,
        })
    }
}

/// Get the schema string used for agent attestations.
pub fn schema_string() -> &'static str {
    SCHEMA_STRING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_uid_computation_is_deterministic() {
        let uid1 = EasClient::compute_schema_uid();
        let uid2 = EasClient::compute_schema_uid();
        assert_eq!(uid1, uid2);
        assert_ne!(uid1, [0u8; 32]);
    }

    #[test]
    fn test_schema_string_is_valid() {
        let s = schema_string();
        assert!(s.contains("agentId"));
        assert!(s.contains("name"));
        assert!(s.contains("owner"));
        assert!(s.contains("manifestCid"));
        assert!(s.contains("capabilities"));
        assert!(s.contains("audited"));
    }

    #[test]
    fn test_eas_client_requires_env() {
        std::env::remove_var("EAS_SCHEMA_UID");
        assert!(EasClient::from_env().is_err());
    }

    #[test]
    fn test_attestation_result_serialization() {
        let result = AttestationResult {
            attestation_uid: "0xabc123".to_string(),
            tx_hash: "0xdef456".to_string(),
            schema_uid: "0x789".to_string(),
            eas_url: "https://base.easscan.org/attestation/view/0xabc123".to_string(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["attestation_uid"], "0xabc123");
        assert_eq!(json["tx_hash"], "0xdef456");
    }
}
