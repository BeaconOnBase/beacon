// ── Agent Manifest Types ────────────────────────────────────────────

export interface AgentsManifest {
  name: string;
  description: string;
  version?: string;
  agent_identity?: string;
  capabilities: Capability[];
  endpoints: Endpoint[];
  authentication?: Authentication;
  rate_limits?: RateLimits;
  contact?: string;
  source_hash?: string;
  zk_proof?: string;
  generation_timestamp?: number;
}

export interface Capability {
  name: string;
  description: string;
  input_schema?: Record<string, unknown>;
  output_schema?: Record<string, unknown>;
  examples: string[];
}

export interface Endpoint {
  path: string;
  method: string;
  description: string;
  parameters: Parameter[];
  x402_enabled?: boolean;
  price_per_call?: string;
  payment_currency?: string;
  payment_network?: string;
}

export interface Parameter {
  name: string;
  type: string;
  required: boolean;
  description: string;
}

export interface Authentication {
  type: string;
  description?: string;
}

export interface RateLimits {
  requests_per_minute?: number;
  requests_per_day?: number;
  notes?: string;
}

// ── Registry Types ──────────────────────────────────────────────────

export interface RegistryEntry {
  agent_id: string;
  name: string;
  description: string;
  basename?: string;
  manifest_cid?: string;
  owner: string;
  wallet_address?: string;
  registered_at: number;
  tx_hash?: string;
}

export interface RegisterRequest {
  name: string;
  description: string;
  basename?: string;
  manifest_json: Record<string, unknown>;
  owner_address: string;
}

export interface RegisterResponse {
  agent_id: string;
  tx_hash?: string;
  registry_url: string;
}

export interface RegistryQuery {
  query?: string;
  owner?: string;
  framework?: string;
  limit?: number;
  offset?: number;
}

// ── A2A Types ───────────────────────────────────────────────────────

export interface DiscoveryQuery {
  capability?: string;
  framework?: string;
  has_attestation?: boolean;
  limit?: number;
  offset?: number;
}

export interface DiscoveredAgent {
  agent_id: string;
  name: string;
  description: string;
  capabilities: string[];
  endpoint_url?: string;
  manifest_cid?: string;
  basename?: string;
  framework?: string;
}

export interface A2AMessage {
  from_agent_id: string;
  to_agent_id: string;
  message_type: string;
  payload: Record<string, unknown>;
  reply_to?: string;
}

export interface A2AMessageResponse {
  message_id: string;
  status: string;
}

export interface StoredMessage {
  id: string;
  from_agent_id: string;
  to_agent_id: string;
  message_type: string;
  payload: Record<string, unknown>;
  reply_to?: string;
  status: string;
  created_at?: string;
}

// ── Generation Types ────────────────────────────────────────────────

export interface GenerateRequest {
  github_url: string;
  provider?: string;
}

export interface GenerateResponse {
  manifest: AgentsManifest;
  agents_md: string;
}

export interface ValidateRequest {
  content: string;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
  endpoint_results: EndpointCheckResult[];
}

export interface EndpointCheckResult {
  endpoint: string;
  reachable: boolean;
  status_code?: number;
  error?: string;
}

// ── x402 Types ──────────────────────────────────────────────────────

export interface PaymentRequirements {
  version: string;
  scheme: string;
  network: string;
  asset: string;
  payTo: string;
  amount: string;
  maxTimeoutSeconds: number;
  resource: string;
  mimeType: string;
  extra: Record<string, unknown>;
}

export interface X402Agent {
  agent_id: string;
  name: string;
  description: string;
  x402_endpoints: Endpoint[];
}

// ── Wallet Types ────────────────────────────────────────────────────

export interface WalletInfo {
  agent_id: string;
  wallet_address: string;
  chain: string;
}

// ── Health Types ────────────────────────────────────────────────────

export interface HealthStatus {
  agent_id: string;
  status: "online" | "offline" | "degraded";
  latency_ms?: number;
  last_checked?: string;
}

// ── Attestation Types ───────────────────────────────────────────────

export interface Attestation {
  id: string;
  agent_id: string;
  attestation_uid: string;
  schema_uid: string;
  tx_hash: string;
  attester: string;
  revoked?: boolean;
  created_at?: string;
}

export interface CreateAttestationRequest {
  agent_id: string;
  schema_uid: string;
  data: Record<string, unknown>;
}

// ── Status Types ────────────────────────────────────────────────────

export interface RegistryStatus {
  total_agents: number;
  online_agents: number;
  total_attestations: number;
  recent_registrations: RegistryEntry[];
}
