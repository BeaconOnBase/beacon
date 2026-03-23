use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// IPFS pinning client using Pinata API.
/// Pins AGENTS.md manifests to IPFS for permanent, decentralized storage.
pub struct IpfsClient {
    jwt: String,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct PinJsonRequest {
    #[serde(rename = "pinataContent")]
    pinata_content: serde_json::Value,
    #[serde(rename = "pinataMetadata")]
    pinata_metadata: PinataMetadata,
}

#[derive(Debug, Serialize)]
struct PinataMetadata {
    name: String,
}

#[derive(Debug, Deserialize)]
struct PinResponse {
    #[serde(rename = "IpfsHash")]
    ipfs_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PinResult {
    pub cid: String,
    pub gateway_url: String,
}

impl IpfsClient {
    /// Create a new IPFS client from PINATA_JWT environment variable.
    pub fn from_env() -> Result<Self> {
        let jwt = std::env::var("PINATA_JWT")
            .context("PINATA_JWT not set")?;
        Ok(Self {
            jwt,
            http: reqwest::Client::new(),
        })
    }

    /// Pin a JSON value to IPFS via Pinata.
    pub async fn pin_json(&self, name: &str, content: &serde_json::Value) -> Result<PinResult> {
        let request = PinJsonRequest {
            pinata_content: content.clone(),
            pinata_metadata: PinataMetadata {
                name: name.to_string(),
            },
        };

        let resp = self.http
            .post("https://api.pinata.cloud/pinning/pinJSONToIPFS")
            .header("Authorization", format!("Bearer {}", self.jwt))
            .json(&request)
            .send()
            .await
            .context("Failed to pin to IPFS")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Pinata API error ({}): {}", status, body);
        }

        let pin_resp: PinResponse = resp.json().await
            .context("Failed to parse Pinata response")?;

        Ok(PinResult {
            gateway_url: gateway_url(&pin_resp.ipfs_hash),
            cid: pin_resp.ipfs_hash,
        })
    }

    /// Pin raw text content (e.g., AGENTS.md) to IPFS via Pinata.
    pub async fn pin_raw(&self, name: &str, content: &str) -> Result<PinResult> {
        let form = reqwest::multipart::Form::new()
            .text("pinataMetadata", serde_json::json!({ "name": name }).to_string())
            .part("file", reqwest::multipart::Part::text(content.to_string())
                .file_name(format!("{}.md", name)));

        let resp = self.http
            .post("https://api.pinata.cloud/pinning/pinFileToIPFS")
            .header("Authorization", format!("Bearer {}", self.jwt))
            .multipart(form)
            .send()
            .await
            .context("Failed to pin file to IPFS")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Pinata API error ({}): {}", status, body);
        }

        let pin_resp: PinResponse = resp.json().await
            .context("Failed to parse Pinata response")?;

        Ok(PinResult {
            gateway_url: gateway_url(&pin_resp.ipfs_hash),
            cid: pin_resp.ipfs_hash,
        })
    }
}

/// Build a public gateway URL for a CID.
pub fn gateway_url(cid: &str) -> String {
    format!("https://gateway.pinata.cloud/ipfs/{}", cid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_url_format() {
        let cid = "QmTest123abc";
        let url = gateway_url(cid);
        assert_eq!(url, "https://gateway.pinata.cloud/ipfs/QmTest123abc");
    }

    #[test]
    fn test_ipfs_client_requires_env() {
        // Without PINATA_JWT set, should fail
        std::env::remove_var("PINATA_JWT");
        assert!(IpfsClient::from_env().is_err());
    }

    #[test]
    fn test_pin_result_serialization() {
        let result = PinResult {
            cid: "QmTest".to_string(),
            gateway_url: "https://gateway.pinata.cloud/ipfs/QmTest".to_string(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["cid"], "QmTest");
        assert!(json["gateway_url"].as_str().unwrap().contains("QmTest"));
    }
}
