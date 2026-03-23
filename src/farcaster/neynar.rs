use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .use_rustls_tls()
        .build()
        .expect("Failed to create reqwest client")
});

const NEYNAR_BASE: &str = "https://api.neynar.com/v2/farcaster";

pub struct NeynarClient {
    pub api_key: String,
    pub signer_uuid: String,
    pub bot_fid: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Cast {
    pub hash: String,
    pub author: CastAuthor,
    pub text: String,
    pub timestamp: String,
    pub parent_hash: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CastAuthor {
    pub fid: u64,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MentionsResponse {
    result: Option<MentionsResult>,
    notifications: Option<Vec<Notification>>,
}

#[derive(Debug, Deserialize)]
struct MentionsResult {
    notifications: Vec<Notification>,
    next: Option<NextCursor>,
}

#[derive(Debug, Deserialize)]
struct Notification {
    cast: Option<Cast>,
    target: Option<NotificationTarget>,
}

#[derive(Debug, Deserialize)]
struct NotificationTarget {
    hash: Option<String>,
}

impl Notification {
    fn get_cast(&self) -> Option<Cast> {
        if let Some(ref cast) = self.cast {
            return Some(cast.clone());
        }
        if let Some(ref target) = self.target {
            if let Some(ref hash) = target.hash {
                return Some(Cast {
                    hash: hash.clone(),
                    author: CastAuthor { fid: 0, username: None },
                    text: String::new(),
                    timestamp: String::new(),
                    parent_hash: None,
                });
            }
        }
        None
    }
}

#[derive(Debug, Deserialize)]
struct NextCursor {
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PostCastResponse {
    cast: PostedCast,
}

#[derive(Debug, Deserialize)]
struct PostedCast {
    hash: String,
}

impl NeynarClient {
    pub fn new(api_key: String, signer_uuid: String, bot_fid: u64) -> Self {
        Self {
            api_key,
            signer_uuid,
            bot_fid,
        }
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("NEYNAR_API_KEY")
            .context("NEYNAR_API_KEY not set")?;
        let signer_uuid = std::env::var("NEYNAR_SIGNER_UUID")
            .context("NEYNAR_SIGNER_UUID not set")?;
        let bot_fid: u64 = std::env::var("FARCASTER_BOT_FID")
            .context("FARCASTER_BOT_FID not set")?
            .parse()
            .context("FARCASTER_BOT_FID must be a number")?;

        Ok(Self::new(api_key, signer_uuid, bot_fid))
    }

    pub async fn fetch_mentions(&self, cursor: Option<&str>) -> Result<(Vec<Cast>, Option<String>)> {
        let mut url = format!(
            "{}/notifications?fid={}&type=mentions",
            NEYNAR_BASE, self.bot_fid
        );
        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let resp = CLIENT
            .get(&url)
            .header("api_key", &self.api_key)
            .send()
            .await
            .context("Failed to fetch mentions from Neynar")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err = resp.text().await.unwrap_or_default();
            anyhow::bail!("Neynar API returned {}: {}", status, err);
        }

        let data: MentionsResponse = resp.json().await?;
        
        let notifications = data
            .result
            .as_ref()
            .map(|r| &r.notifications)
            .or(data.notifications.as_ref())
            .map(|n| n.as_slice())
            .unwrap_or(&[]);

        let casts: Vec<Cast> = notifications
            .iter()
            .filter_map(|n| n.get_cast())
            .collect();

        let next_cursor = data.result.as_ref().and_then(|r| r.next.as_ref().and_then(|n| n.cursor.as_ref())).cloned();
        Ok((casts, next_cursor))
    }

    pub async fn post_cast(
        &self,
        text: &str,
        parent_hash: Option<&str>,
        channel_id: Option<&str>,
    ) -> Result<String> {
        self.post_cast_with_embeds(text, parent_hash, channel_id, None).await
    }

    pub async fn post_cast_with_embeds(
        &self,
        text: &str,
        parent_hash: Option<&str>,
        channel_id: Option<&str>,
        embeds: Option<Vec<String>>,
    ) -> Result<String> {
        let mut body = json!({
            "signer_uuid": self.signer_uuid,
            "text": text,
        });

        if let Some(parent) = parent_hash {
            body["parent"] = json!(parent);
        }
        if let Some(channel) = channel_id {
            body["channel_id"] = json!(channel);
        }
        if let Some(embed_urls) = embeds {
            body["embeds"] = json!(
                embed_urls.iter().map(|u| json!({"url": u})).collect::<Vec<_>>()
            );
        }

        let resp = CLIENT
            .post(&format!("{}/cast", NEYNAR_BASE))
            .header("api_key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to post cast via Neynar")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Neynar post_cast returned {}: {}", status, err_body);
        }

        let data: PostCastResponse = resp.json().await?;
        Ok(data.cast.hash)
    }

    pub async fn post_threaded(
        &self,
        chunks: &[String],
        first_parent_hash: &str,
        channel_id: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut hashes = Vec::new();
        let mut parent = first_parent_hash.to_string();

        for chunk in chunks {
            let hash = self.post_cast(chunk, Some(&parent), channel_id).await?;
            parent = hash.clone();
            hashes.push(hash);
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        Ok(hashes)
    }
}

pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let max = if max_chars == 0 { 1024 } else { max_chars };
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if !current.is_empty() && current.len() + line.len() + 1 > max {
            chunks.push(current.clone());
            current.clear();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        if line.len() > max {
            let mut remaining = line;
            while !remaining.is_empty() {
                let end = remaining.char_indices().nth(max).map(|(i, _)| i).unwrap_or(remaining.len());
                if !current.is_empty() {
                    chunks.push(current.clone());
                    current.clear();
                }
                current.push_str(&remaining[..end]);
                remaining = &remaining[end..];
            }
        } else {
            current.push_str(line);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}
