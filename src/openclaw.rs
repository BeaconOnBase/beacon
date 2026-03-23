use crate::models::{AgentFramework, RepoContext};

/// Agent framework identifiers to detect in package manifests
const FRAMEWORK_PATTERNS: &[(&str, &str)] = &[
    ("@openclaw/sdk", "OpenClaw"),
    ("openclaw", "OpenClaw"),
    ("@coinbase/agentkit", "AgentKit"),
    ("coinbase-agentkit", "AgentKit"),
    ("langchain", "LangChain"),
    ("crewai", "CrewAI"),
    ("autogpt", "AutoGPT"),
    ("@ai16z/eliza", "Eliza"),
    ("eliza-core", "Eliza"),
];

const OPENCLAW_CONFIG_FILES: &[&str] = &[
    "openclaw.json",
    "openclaw.yaml",
    "openclaw.yml",
    ".openclaw/config.json",
    ".openclaw/config.yaml",
];

/// Detect agent frameworks from repo context
pub fn detect_framework(ctx: &RepoContext) -> Option<AgentFramework> {
    // Check package manifest for framework dependencies
    if let Some(manifest) = &ctx.package_manifest {
        let manifest_lower = manifest.to_lowercase();
        for (pattern, name) in FRAMEWORK_PATTERNS {
            if manifest_lower.contains(pattern) {
                let version = extract_version(&manifest_lower, pattern);
                let mut framework = AgentFramework {
                    name: name.to_string(),
                    version,
                    config_files: vec![],
                    detected_capabilities: vec![],
                };

                // Check for config files in source files
                for file in &ctx.source_files {
                    let path_lower = file.path.to_lowercase();
                    for config in OPENCLAW_CONFIG_FILES {
                        if path_lower.contains(config) {
                            framework.config_files.push(file.path.clone());
                        }
                    }

                    // Detect capabilities from source code patterns
                    detect_capabilities_from_source(&file.content, &mut framework);
                }

                return Some(framework);
            }
        }
    }

    // Check source files for framework imports even without manifest entry
    // Only match actual import statements, not references in config/detection code
    for file in &ctx.source_files {
        let content = &file.content;
        for (pattern, name) in FRAMEWORK_PATTERNS {
            // Match real import patterns: import ... from 'pattern' or require('pattern') or use pattern::
            let has_js_import = content.contains(&format!("from '{}'", pattern))
                || content.contains(&format!("from \"{}\"", pattern))
                || content.contains(&format!("require('{}')", pattern))
                || content.contains(&format!("require(\"{}\")", pattern));
            let has_python_import = content.contains(&format!("import {}", pattern))
                || content.contains(&format!("from {} import", pattern));

            if has_js_import || has_python_import {
                let mut framework = AgentFramework {
                    name: name.to_string(),
                    version: None,
                    config_files: vec![],
                    detected_capabilities: vec![],
                };
                detect_capabilities_from_source(content, &mut framework);
                return Some(framework);
            }
        }
    }

    None
}

fn detect_capabilities_from_source(content: &str, framework: &mut AgentFramework) {
    let patterns = [
        ("transfer", "token_transfer"),
        ("swap", "token_swap"),
        ("deploy_contract", "contract_deployment"),
        ("deploy", "contract_deployment"),
        ("sign_message", "message_signing"),
        ("send_transaction", "transaction_sending"),
        ("get_balance", "balance_query"),
        ("create_wallet", "wallet_creation"),
        ("register_action", "custom_action"),
        ("tool_call", "tool_execution"),
        ("run_agent", "agent_orchestration"),
    ];

    let content_lower = content.to_lowercase();
    for (pattern, capability) in &patterns {
        if content_lower.contains(pattern) && !framework.detected_capabilities.contains(&capability.to_string()) {
            framework.detected_capabilities.push(capability.to_string());
        }
    }
}

fn extract_version(manifest: &str, package: &str) -> Option<String> {
    // Try to find version pattern like "package": "^1.0.0" or package = "1.0.0"
    if let Some(idx) = manifest.find(package) {
        let after = &manifest[idx + package.len()..];
        // Look for version string pattern
        if let Some(start) = after.find(|c: char| c.is_ascii_digit()) {
            let version_str: String = after[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect();
            if !version_str.is_empty() {
                return Some(version_str);
            }
        }
    }
    None
}
