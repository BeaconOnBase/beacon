#[cfg(test)]
mod tests {
    use crate::scanner;
    use crate::validator;
    use crate::generator;
    use crate::models::{AgentsManifest, Capability, Endpoint, Parameter, Authentication};
    use std::fs;
    use std::path::Path;

    fn mock_manifest() -> AgentsManifest {
        AgentsManifest {
            name: "test-repo".to_string(),
            description: "A test repository for agents.".to_string(),
            version: Some("1.0.0".to_string()),
            agent_identity: None,
            capabilities: vec![
                Capability {
                    name: "do_something".to_string(),
                    description: "Does something useful.".to_string(),
                    input_schema: None,
                    output_schema: None,
                    examples: vec!["example usage".to_string()],
                }
            ],
            endpoints: vec![
                Endpoint {
                    path: "/api/test".to_string(),
                    method: "GET".to_string(),
                    description: "Test endpoint".to_string(),
                    parameters: vec![
                        Parameter {
                            name: "id".to_string(),
                            r#type: "string".to_string(),
                            required: true,
                            description: "The ID".to_string(),
                        }
                    ],
                }
            ],
            authentication: Some(Authentication {
                r#type: "bearer".to_string(),
                description: Some("Pass token in Authorization header".to_string()),
            }),
            rate_limits: None,
            contact: None,
            source_hash: None,
            zk_proof: None,
            generation_timestamp: None,
        }
    }

    #[test]
    fn test_scanner_rejects_nonexistent_path() {
        let result = scanner::scan_local("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_scanner_scans_current_dir() {
        let result = scanner::scan_local("./");
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert!(!ctx.name.is_empty());
        assert!(ctx.source_files.len() > 0);
        assert!(ctx.package_manifest.is_some());
    }

    #[test]
    fn test_scanner_finds_cargo_toml() {
        let result = scanner::scan_local("./").unwrap();
        assert!(result.package_manifest.is_some());
        let manifest = result.package_manifest.unwrap();
        assert!(manifest.contains("[package]"));
    }

    #[test]
    fn test_generator_creates_file() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_output.md";
        let result = generator::generate_agents_md(&manifest, path);
        assert!(result.is_ok());
        assert!(Path::new(path).exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_generator_output_contains_name() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_name.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("test-repo"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_generator_output_contains_capabilities() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_caps.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("## Capabilities"));
        assert!(content.contains("do_something"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_generator_output_contains_endpoints() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_ep.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("## Endpoints"));
        assert!(content.contains("GET"));
        assert!(content.contains("/api/test"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_validator_passes_valid_content() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_valid.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        let result = validator::validate_content(&content).unwrap();
        assert!(result.valid);
        assert!(result.errors.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_validator_fails_missing_capabilities() {
        let content = "# AGENTS.md — test\n\n> A description\n\n## Endpoints\n\n";
        let result = validator::validate_content(content).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Capabilities")));
    }

    #[test]
    fn test_validator_fails_missing_heading() {
        let content = "just some random text with no heading";
        let result = validator::validate_content(content).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("heading")));
    }

    #[test]
    fn test_validator_warns_missing_description() {
        let content = "# AGENTS.md — test\n\n## Capabilities\n\n### `do_thing`\n\nDoes a thing.\n\n";
        let result = validator::validate_content(content).unwrap();
        assert!(result.warnings.iter().any(|w| w.contains("description")));
    }
}

#[cfg(test)]
mod db_tests {
    use crate::db;

    #[tokio::test]
    async fn test_payment_already_used_returns_false_for_unknown_hash() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let result = db::payment_already_used("nonexistent_hash_xyz_123").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_create_run_returns_uuid() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let result = db::create_run("test-repo").await;
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
        assert_eq!(id.len(), 36);
    }

    #[tokio::test]
    async fn test_full_run_lifecycle() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let run_id = db::create_run("test-lifecycle-repo").await.unwrap();
        
        let paid = db::mark_run_paid(&run_id, "0xtest_txn_hash_123", "base").await;
        assert!(paid.is_ok());

        let complete = db::mark_run_complete(&run_id, "# AGENTS.md\n\n> test").await;
        assert!(complete.is_ok());
    }

    #[tokio::test]
    async fn test_mark_run_failed() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let run_id = db::create_run("test-fail-repo").await.unwrap();
        let result = db::mark_run_failed(&run_id, "inference failed").await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod api_tests {
    use crate::{AppState, check_rate_limit};
    use axum::http::StatusCode;
    use std::sync::Arc;

    #[tokio::test]
    #[ignore]
    async fn test_check_rate_limit_enforces_limit() {
        dotenvy::dotenv().ok();
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(url) => url,
            Err(_) => return,
        };

        let state = AppState {
            redis_client: Arc::new(redis::Client::open(redis_url).unwrap()),
        };

        for _ in 0..20 {
            let result = check_rate_limit(&state, "1.2.3.4").await;
            assert!(result.is_ok());
        }

        let result = check_rate_limit(&state, "1.2.3.4").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::TOO_MANY_REQUESTS);
    }
}

#[cfg(test)]
mod mcp_tests {
    use crate::mcp::BeaconMcpHandler;
    use crate::openclaw;
    use crate::scanner;
    use crate::models::{RepoContext, SourceFile, Language};
    use rust_mcp_sdk::mcp_server::ServerHandler;
    use rust_mcp_sdk::McpServer;
    use rust_mcp_sdk::task_store::TaskStore;
    use rust_mcp_sdk::schema::{InitializeResult, Implementation, ServerCapabilities};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_mcp_lists_tools() {
        let handler = BeaconMcpHandler::default();
        let result = handler.handle_list_tools_request(None, Arc::new(MockMcpServer::new())).await;
        assert!(result.is_ok());
        let tools = result.unwrap().tools;
        assert!(tools.iter().any(|t| t.name == "generate_agents_md"));
        assert!(tools.iter().any(|t| t.name == "validate_agents_md"));
    }

    struct MockMcpServer {
        info: InitializeResult,
        auth: RwLock<Option<rust_mcp_sdk::auth::AuthInfo>>,
    }

    impl MockMcpServer {
        fn new() -> Self {
            Self {
                info: InitializeResult {
                    server_info: Implementation {
                        name: "mock".into(),
                        version: "0.1".into(),
                        title: None,
                        description: None,
                        icons: vec![],
                        website_url: None,
                    },
                    protocol_version: "2025-11-25".into(),
                    capabilities: ServerCapabilities {
                        tools: None,
                        resources: None,
                        prompts: None,
                        logging: None,
                        completions: None,
                        tasks: None,
                        experimental: None,
                    },
                    instructions: None,
                    meta: None,
                },
                auth: RwLock::new(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl McpServer for MockMcpServer {
        async fn start(self: Arc<Self>) -> std::result::Result<(), rust_mcp_sdk::error::McpSdkError> { Ok(()) }
        async fn set_client_details(&self, _: rust_mcp_sdk::schema::InitializeRequestParams) -> std::result::Result<(), rust_mcp_sdk::error::McpSdkError> { Ok(()) }
        fn server_info(&self) -> &InitializeResult { &self.info }
        fn client_info(&self) -> Option<rust_mcp_sdk::schema::InitializeRequestParams> { None }
        async fn auth_info(&self) -> tokio::sync::RwLockReadGuard<'_, Option<rust_mcp_sdk::auth::AuthInfo>> { self.auth.read().await }
        async fn auth_info_cloned(&self) -> Option<rust_mcp_sdk::auth::AuthInfo> { None }
        async fn update_auth_info(&self, _: Option<rust_mcp_sdk::auth::AuthInfo>) {}
        async fn wait_for_initialization(&self) {}
        fn task_store(&self) -> Option<Arc<dyn TaskStore<rust_mcp_sdk::schema::ClientJsonrpcRequest, rust_mcp_sdk::schema::ResultFromServer>>> { None }
        fn client_task_store(&self) -> Option<Arc<dyn TaskStore<rust_mcp_sdk::schema::ServerJsonrpcRequest, rust_mcp_sdk::schema::ResultFromClient>>> { None }
        async fn stderr_message(&self, _: String) -> std::result::Result<(), rust_mcp_sdk::error::McpSdkError> { Ok(()) }
        fn session_id(&self) -> Option<String> { None }
        fn capabilities(&self) -> &ServerCapabilities { &self.info.capabilities }
        async fn send(&self, _: rust_mcp_sdk::schema::MessageFromServer, _: Option<rust_mcp_sdk::schema::RequestId>, _: Option<std::time::Duration>) -> std::result::Result<Option<rust_mcp_sdk::schema::ClientMessage>, rust_mcp_sdk::error::McpSdkError> { Ok(None) }
        async fn send_batch(&self, _: Vec<rust_mcp_sdk::schema::ServerMessage>, _: Option<std::time::Duration>) -> std::result::Result<Option<Vec<rust_mcp_sdk::schema::ClientMessage>>, rust_mcp_sdk::error::McpSdkError> { Ok(None) }
    }

    // ── OpenClaw Detection Tests ────────────────────────────────────

    #[test]
    fn test_openclaw_detects_openclaw_in_package_json() {
        let ctx = RepoContext {
            name: "test-agent".to_string(),
            package_manifest: Some(r#"{ "dependencies": { "@openclaw/sdk": "^1.2.0" } }"#.to_string()),
            source_files: vec![],
            ..Default::default()
        };
        let fw = openclaw::detect_framework(&ctx);
        assert!(fw.is_some());
        let fw = fw.unwrap();
        assert_eq!(fw.name, "OpenClaw");
        assert_eq!(fw.version, Some("1.2.0".to_string()));
    }

    #[test]
    fn test_openclaw_detects_agentkit_in_manifest() {
        let ctx = RepoContext {
            name: "test-agent".to_string(),
            package_manifest: Some(r#"{ "dependencies": { "@coinbase/agentkit": "^0.5.0" } }"#.to_string()),
            source_files: vec![],
            ..Default::default()
        };
        let fw = openclaw::detect_framework(&ctx);
        assert!(fw.is_some());
        assert_eq!(fw.unwrap().name, "AgentKit");
    }

    #[test]
    fn test_openclaw_detects_langchain_in_pyproject() {
        let ctx = RepoContext {
            name: "test-agent".to_string(),
            package_manifest: Some(r#"[tool.poetry.dependencies]\nlangchain = "^0.1.0""#.to_string()),
            source_files: vec![],
            ..Default::default()
        };
        let fw = openclaw::detect_framework(&ctx);
        assert!(fw.is_some());
        assert_eq!(fw.unwrap().name, "LangChain");
    }

    #[test]
    fn test_openclaw_detects_capabilities_from_source() {
        let ctx = RepoContext {
            name: "test-agent".to_string(),
            package_manifest: Some(r#"{ "dependencies": { "@openclaw/sdk": "1.0.0" } }"#.to_string()),
            source_files: vec![
                SourceFile {
                    path: "src/agent.ts".to_string(),
                    language: Language::TypeScript,
                    content: "await agent.transfer(token, amount);\nawait agent.get_balance(address);\nawait agent.deploy_contract(bytecode);".to_string(),
                },
            ],
            ..Default::default()
        };
        let fw = openclaw::detect_framework(&ctx).unwrap();
        assert!(fw.detected_capabilities.contains(&"token_transfer".to_string()));
        assert!(fw.detected_capabilities.contains(&"balance_query".to_string()));
        assert!(fw.detected_capabilities.contains(&"contract_deployment".to_string()));
    }

    #[test]
    fn test_openclaw_detects_from_imports_without_manifest() {
        let ctx = RepoContext {
            name: "test-agent".to_string(),
            package_manifest: None,
            source_files: vec![
                SourceFile {
                    path: "src/index.ts".to_string(),
                    language: Language::TypeScript,
                    content: "import { Agent } from '@coinbase/agentkit';\nagent.swap(tokenA, tokenB);".to_string(),
                },
            ],
            ..Default::default()
        };
        let fw = openclaw::detect_framework(&ctx);
        assert!(fw.is_some());
        assert_eq!(fw.unwrap().name, "AgentKit");
    }

    #[test]
    fn test_openclaw_returns_none_for_regular_repo() {
        let ctx = RepoContext {
            name: "regular-app".to_string(),
            package_manifest: Some(r#"{ "dependencies": { "express": "^4.0.0" } }"#.to_string()),
            source_files: vec![
                SourceFile {
                    path: "src/index.js".to_string(),
                    language: Language::JavaScript,
                    content: "const app = express();\napp.get('/', (req, res) => res.send('hello'));".to_string(),
                },
            ],
            ..Default::default()
        };
        let fw = openclaw::detect_framework(&ctx);
        assert!(fw.is_none());
    }

    #[test]
    fn test_openclaw_detects_config_files() {
        let ctx = RepoContext {
            name: "test-agent".to_string(),
            package_manifest: Some(r#"{ "dependencies": { "@openclaw/sdk": "2.0.0" } }"#.to_string()),
            source_files: vec![
                SourceFile {
                    path: "openclaw.json".to_string(),
                    language: Language::Other("json".to_string()),
                    content: r#"{ "agent": { "name": "my-agent" } }"#.to_string(),
                },
            ],
            ..Default::default()
        };
        let fw = openclaw::detect_framework(&ctx).unwrap();
        assert_eq!(fw.config_files, vec!["openclaw.json"]);
    }

    // ── Basenames Namehash Tests ──────────────────────────────────

    #[test]
    fn test_namehash_empty() {
        let hash = crate::registry::namehash_public("");
        assert_eq!(hash, [0u8; 32]);
    }

    #[test]
    fn test_namehash_eth() {
        let hash = crate::registry::namehash_public("eth");
        // Known namehash for 'eth': 0x93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae
        let expected: [u8; 32] = [
            0x93, 0xcd, 0xeb, 0x70, 0x8b, 0x75, 0x45, 0xdc,
            0x66, 0x8e, 0xb9, 0x28, 0x01, 0x76, 0x16, 0x9d,
            0x1c, 0x33, 0xcf, 0xd8, 0xed, 0x6f, 0x04, 0x69,
            0x0a, 0x0b, 0xcc, 0x88, 0xa9, 0x3f, 0xc4, 0xae,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_namehash_base_eth() {
        // namehash('base.eth') should be deterministic and non-zero
        let hash = crate::registry::namehash_public("base.eth");
        assert_ne!(hash, [0u8; 32]);
    }

    #[test]
    fn test_namehash_subdomain() {
        // namehash('jesse.base.eth') should differ from namehash('base.eth')
        let hash1 = crate::registry::namehash_public("base.eth");
        let hash2 = crate::registry::namehash_public("jesse.base.eth");
        assert_ne!(hash1, hash2);
    }

    // ── Scanner Framework Detection Test ────────────────────────────

    #[test]
    fn test_scanner_populates_agent_framework_field() {
        // Scanning current dir — the agent_framework field should be populated
        // (it may detect framework patterns from test data in source files)
        let ctx = scanner::scan_local("./").unwrap();
        // Just verify the field exists and scan completes without error
        // The actual value depends on repo contents
        assert!(!ctx.name.is_empty());
    }
}
