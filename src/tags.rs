#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db;

// ── Agent Tags & Categories ─────────────────────────────────────────
//
// Agents can tag themselves with categories (DeFi, NFT, security, etc.)
// for better discovery and filtering beyond capability search.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentTag {
    pub id: String,
    pub agent_id: String,
    pub tag: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct TagQuery {
    pub tag: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct TagUpdateRequest {
    pub tags: Vec<String>,
}

// Predefined categories (agents can also use custom tags)
pub const CATEGORY_DEFI: &str = "defi";
pub const CATEGORY_NFT: &str = "nft";
pub const CATEGORY_SECURITY: &str = "security";
pub const CATEGORY_ANALYTICS: &str = "analytics";
pub const CATEGORY_INFRASTRUCTURE: &str = "infrastructure";
pub const CATEGORY_SOCIAL: &str = "social";
pub const CATEGORY_GAMING: &str = "gaming";
pub const CATEGORY_DAO: &str = "dao";
pub const CATEGORY_BRIDGE: &str = "bridge";
pub const CATEGORY_ORACLE: &str = "oracle";
pub const CATEGORY_STORAGE: &str = "storage";
pub const CATEGORY_IDENTITY: &str = "identity";

pub const PREDEFINED_CATEGORIES: &[&str] = &[
    CATEGORY_DEFI, CATEGORY_NFT, CATEGORY_SECURITY, CATEGORY_ANALYTICS,
    CATEGORY_INFRASTRUCTURE, CATEGORY_SOCIAL, CATEGORY_GAMING, CATEGORY_DAO,
    CATEGORY_BRIDGE, CATEGORY_ORACLE, CATEGORY_STORAGE, CATEGORY_IDENTITY,
];

const MAX_TAGS_PER_AGENT: usize = 10;
const MAX_TAG_LENGTH: usize = 50;

pub struct AgentTags;

impl AgentTags {
    /// Set tags for an agent (replaces existing tags)
    pub async fn set_tags(agent_id: &str, tags: &[String]) -> Result<Vec<AgentTag>> {
        // Validate
        if tags.len() > MAX_TAGS_PER_AGENT {
            anyhow::bail!("Maximum {} tags per agent", MAX_TAGS_PER_AGENT);
        }

        let normalized: Vec<String> = tags.iter()
            .map(|t| Self::normalize_tag(t))
            .collect::<Result<Vec<_>>>()?;

        // Remove duplicates
        let mut unique_tags: Vec<String> = Vec::new();
        for tag in &normalized {
            if !unique_tags.contains(tag) {
                unique_tags.push(tag.clone());
            }
        }

        // Replace all tags
        db::replace_agent_tags(agent_id, &unique_tags).await?;

        // Return the new tag list
        db::get_agent_tags(agent_id).await
    }

    /// Add tags to an agent (keeps existing)
    pub async fn add_tags(agent_id: &str, tags: &[String]) -> Result<Vec<AgentTag>> {
        let existing = db::get_agent_tags(agent_id).await?;
        let existing_names: Vec<String> = existing.iter().map(|t| t.tag.clone()).collect();

        let mut new_tags = existing_names.clone();
        for tag in tags {
            let normalized = Self::normalize_tag(tag)?;
            if !new_tags.contains(&normalized) {
                new_tags.push(normalized);
            }
        }

        if new_tags.len() > MAX_TAGS_PER_AGENT {
            anyhow::bail!("Would exceed maximum of {} tags per agent", MAX_TAGS_PER_AGENT);
        }

        db::replace_agent_tags(agent_id, &new_tags).await?;
        db::get_agent_tags(agent_id).await
    }

    /// Remove specific tags from an agent
    pub async fn remove_tags(agent_id: &str, tags: &[String]) -> Result<Vec<AgentTag>> {
        for tag in tags {
            let normalized = Self::normalize_tag(tag)?;
            db::delete_agent_tag(agent_id, &normalized).await?;
        }
        db::get_agent_tags(agent_id).await
    }

    /// Get all tags for an agent
    pub async fn get_tags(agent_id: &str) -> Result<Vec<AgentTag>> {
        db::get_agent_tags(agent_id).await
    }

    /// Search agents by tag
    pub async fn search_by_tag(tag: &str, limit: usize, offset: usize) -> Result<Vec<String>> {
        let normalized = Self::normalize_tag(tag)?;
        db::get_agents_by_tag(&normalized, limit, offset).await
    }

    /// Get popular tags with counts
    pub async fn get_popular_tags(limit: usize) -> Result<Vec<TagCount>> {
        db::get_popular_tags(limit).await
    }

    /// Get all predefined categories
    pub fn get_categories() -> Vec<String> {
        PREDEFINED_CATEGORIES.iter().map(|s| s.to_string()).collect()
    }

    /// Normalize a tag: lowercase, trim, validate length
    fn normalize_tag(tag: &str) -> Result<String> {
        let normalized = tag.trim().to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "");

        if normalized.is_empty() {
            anyhow::bail!("Tag cannot be empty");
        }
        if normalized.len() > MAX_TAG_LENGTH {
            anyhow::bail!("Tag '{}' exceeds maximum length of {} characters", normalized, MAX_TAG_LENGTH);
        }

        Ok(normalized)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_tag() {
        assert_eq!(AgentTags::normalize_tag("DeFi").unwrap(), "defi");
        assert_eq!(AgentTags::normalize_tag("  NFT  ").unwrap(), "nft");
        assert_eq!(AgentTags::normalize_tag("smart-contracts").unwrap(), "smart-contracts");
        assert_eq!(AgentTags::normalize_tag("web_3").unwrap(), "web_3");
    }

    #[test]
    fn test_normalize_tag_strips_special_chars() {
        assert_eq!(AgentTags::normalize_tag("DeFi!!!").unwrap(), "defi");
        assert_eq!(AgentTags::normalize_tag("@security#").unwrap(), "security");
    }

    #[test]
    fn test_normalize_tag_empty_fails() {
        assert!(AgentTags::normalize_tag("").is_err());
        assert!(AgentTags::normalize_tag("   ").is_err());
        assert!(AgentTags::normalize_tag("!!!").is_err());
    }

    #[test]
    fn test_normalize_tag_too_long_fails() {
        let long_tag = "a".repeat(51);
        assert!(AgentTags::normalize_tag(&long_tag).is_err());
    }

    #[test]
    fn test_predefined_categories() {
        let cats = AgentTags::get_categories();
        assert_eq!(cats.len(), 12);
        assert!(cats.contains(&"defi".to_string()));
        assert!(cats.contains(&"nft".to_string()));
        assert!(cats.contains(&"security".to_string()));
        assert!(cats.contains(&"identity".to_string()));
    }

    #[test]
    fn test_tag_count_serialization() {
        let tc = TagCount {
            tag: "defi".to_string(),
            count: 42,
        };
        let json = serde_json::to_value(&tc).unwrap();
        assert_eq!(json["tag"], "defi");
        assert_eq!(json["count"], 42);
    }
}
