use std::fs;
use anyhow::Result;
use crate::models::{AgentsManifest, AgentCard, AgentSkill, AgentCardCapabilities};

pub fn generate_agents_md(manifest: &AgentsManifest, output_path: &str) -> Result<()> {
    let content = render_markdown(manifest);
    fs::write(output_path, &content)?;
    println!("   ✓ Written to {}", output_path);

    // Generate Google A2A Agent Card (agent-card.json)
    let card = AgentCard {
        protocol_version: "1.0.0".to_string(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        version: manifest.version.clone().unwrap_or_else(|| "0.1.0".into()),
        url: "https://api.beacon-ai.com/v1/agent".to_string(), // placeholder
        icon_url: None,
        provider: None,
        capabilities: AgentCardCapabilities {
            streaming: true,
            push_notifications: false,
        },
        skills: manifest.capabilities.iter().map(|c| AgentSkill {
            name: c.name.clone(),
            description: c.description.clone(),
            input_schema: c.input_schema.clone(),
            output_schema: c.output_schema.clone(),
        }).collect(),
        security_schemes: serde_json::json!({}),
    };

    let card_json = serde_json::to_string_pretty(&card)?;
    let card_path = output_path.replace("AGENTS.md", "agent-card.json");
    fs::write(&card_path, card_json)?;
    println!("   ✓ Generated Google A2A Agent Card: {}", card_path);

    Ok(())
}

pub fn render_markdown(m: &AgentsManifest) -> String {
    let mut out = String::new();

    out.push_str(&format!("# AGENTS.md — {}\n\n", m.name));
    out.push_str(&format!("> {}\n\n", m.description));

    if let Some(version) = &m.version {
        out.push_str(&format!("**Version:** {}\n\n", version));
    }

    if let Some(hash) = &m.source_hash {
        out.push_str(&format!("**Source Hash:** `{}`\n\n", hash));
    }

    if let Some(ts) = m.generation_timestamp {
        let dt = chrono::DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()).unwrap_or_default();
        out.push_str(&format!("**Generated At:** {}\n\n", dt));
    }

    out.push_str("---\n\n");

    if let Some(proof) = &m.zk_proof {
        out.push_str("## 🛡 Verifiable Generation (ZK Proof)\n\n");
        out.push_str("This AGENTS.md was generated with a zero-knowledge proof attesting that it faithfully represents the repository state at the specified source hash.\n\n");
        out.push_str("<details>\n<summary>View ZK Proof (SP1)</summary>\n\n");
        out.push_str("```\n");
        out.push_str(proof);
        out.push_str("\n```\n\n");
        out.push_str("</details>\n\n---\n\n");
    }

    if let Some(auth) = &m.authentication {
        out.push_str("## Authentication\n\n");
        out.push_str(&format!("**Type:** `{}`\n\n", auth.r#type));
        if let Some(desc) = &auth.description {
            out.push_str(&format!("{}\n\n", desc));
        }
        out.push_str("---\n\n");
    }

    if !m.capabilities.is_empty() {
        out.push_str("## Capabilities\n\n");
        out.push_str("What an agent can do with this repository:\n\n");

        for cap in &m.capabilities {
            out.push_str(&format!("### `{}`\n\n", cap.name));
            out.push_str(&format!("{}\n\n", cap.description));

            if let Some(input) = &cap.input_schema {
                out.push_str("**Input:**\n\n");
                out.push_str("```json\n");
                out.push_str(&serde_json::to_string_pretty(input).unwrap_or_default());
                out.push_str("\n```\n\n");
            }

            if let Some(output) = &cap.output_schema {
                out.push_str("**Output:**\n\n");
                out.push_str("```json\n");
                out.push_str(&serde_json::to_string_pretty(output).unwrap_or_default());
                out.push_str("\n```\n\n");
            }

            if !cap.examples.is_empty() {
                out.push_str("**Examples:**\n\n");
                for ex in &cap.examples {
                    out.push_str(&format!("- {}\n", ex));
                }
                out.push('\n');
            }
        }

        out.push_str("---\n\n");
    }

    if !m.endpoints.is_empty() {
        out.push_str("## Endpoints\n\n");

        for ep in &m.endpoints {
            out.push_str(&format!(
                "### `{} {}`\n\n{}\n\n",
                ep.method.to_uppercase(),
                ep.path,
                ep.description
            ));

            if !ep.parameters.is_empty() {
                out.push_str("| Parameter | Type | Required | Description |\n");
                out.push_str("|-----------|------|----------|-------------|\n");
                for p in &ep.parameters {
                    out.push_str(&format!(
                        "| `{}` | `{}` | {} | {} |\n",
                        p.name,
                        p.r#type,
                        if p.required { "✅" } else { "❌" },
                        p.description
                    ));
                }
                out.push('\n');
            }
        }

        out.push_str("---\n\n");
    }



    if let Some(rl) = &m.rate_limits {
        out.push_str("## Rate Limits\n\n");
        if let Some(rpm) = rl.requests_per_minute {
            out.push_str(&format!("- **Per minute:** {}\n", rpm));
        }
        if let Some(rpd) = rl.requests_per_day {
            out.push_str(&format!("- **Per day:** {}\n", rpd));
        }
        if let Some(notes) = &rl.notes {
            out.push_str(&format!("\n{}\n", notes));
        }
        out.push_str("\n---\n\n");
    }

    
    if let Some(contact) = &m.contact {
        out.push_str(&format!("## Contact\n\n{}\n\n---\n\n", contact));
    }

    // Footer
    out.push_str("*Generated by [Beacon](https://github.com/BeaconOnBase/beacon) — Make any repo agent-ready. Instantly.*\n");

    out
}