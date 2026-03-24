use anyhow::{Result, Context};
use git2::Repository;
use crate::models::AgentsManifest;

/// Zero-Knowledge Verifiable Generation using SP1.
pub struct ZKGenerator;

impl ZKGenerator {
    /// Computes the Git Hash of the current repository.
    pub fn get_repo_hash(path: &str) -> Result<String> {
        let repo = Repository::open(path).context("Failed to open git repository")?;
        let head = repo.head()?.peel_to_commit()?;
        Ok(head.id().to_string())
    }

    /// Generates a real SP1 proof for the manifest generation.
    pub async fn generate_proof(
        _manifest: &AgentsManifest,
        _repo_hash: &str,
    ) -> Result<String> {
        println!("   🛡 Generating ZK Proof via SP1...");
        
        #[cfg(feature = "sp1")]
        {
            use sp1_sdk::{ProverClient, SP1Stdin};
            
            let mut stdin = SP1Stdin::new();
            stdin.write(&_repo_hash);
            stdin.write(&serde_json::to_string(_manifest)?);

            let client = ProverClient::new();
            let (pk, _vk) = client.setup(include_bytes!("../elf/beacon-program-elf"));
            let proof = client.prove(&pk, stdin).run()?;
            
            return Ok(hex::encode(proof.proof));
        }

        #[cfg(not(feature = "sp1"))]
        {
            anyhow::bail!("SP1 feature not enabled. Real ZK proofs require the 'sp1' feature and the SP1 toolchain.")
        }
    }
}
