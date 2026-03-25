from __future__ import annotations

from typing import Any, Optional
from pydantic import BaseModel, Field


# ── Agent Manifest ───────────────────────────────────────────────────

class Capability(BaseModel):
    name: str
    description: str
    input_schema: Optional[dict[str, Any]] = None
    output_schema: Optional[dict[str, Any]] = None
    examples: list[str] = Field(default_factory=list)


class Parameter(BaseModel):
    name: str
    type: str
    required: bool
    description: str


class Endpoint(BaseModel):
    path: str
    method: str
    description: str
    parameters: list[Parameter] = Field(default_factory=list)
    x402_enabled: bool = False
    price_per_call: Optional[str] = None
    payment_currency: Optional[str] = None
    payment_network: Optional[str] = None


class Authentication(BaseModel):
    type: str
    description: Optional[str] = None


class RateLimits(BaseModel):
    requests_per_minute: Optional[int] = None
    requests_per_day: Optional[int] = None
    notes: Optional[str] = None


class AgentsManifest(BaseModel):
    name: str
    description: str
    version: Optional[str] = None
    agent_identity: Optional[str] = None
    capabilities: list[Capability] = Field(default_factory=list)
    endpoints: list[Endpoint] = Field(default_factory=list)
    authentication: Optional[Authentication] = None
    rate_limits: Optional[RateLimits] = None
    contact: Optional[str] = None
    source_hash: Optional[str] = None
    zk_proof: Optional[str] = None
    generation_timestamp: Optional[int] = None


# ── Registry ─────────────────────────────────────────────────────────

class RegistryEntry(BaseModel):
    agent_id: str
    name: str
    description: str
    basename: Optional[str] = None
    manifest_cid: Optional[str] = None
    owner: str = ""
    wallet_address: Optional[str] = None
    registered_at: int = 0
    tx_hash: Optional[str] = None


class RegisterRequest(BaseModel):
    name: str
    description: str
    basename: Optional[str] = None
    manifest_json: dict[str, Any]
    owner_address: str


class RegisterResponse(BaseModel):
    agent_id: str
    tx_hash: Optional[str] = None
    registry_url: str


# ── A2A ──────────────────────────────────────────────────────────────

class DiscoveredAgent(BaseModel):
    agent_id: str
    name: str
    description: str
    capabilities: list[str] = Field(default_factory=list)
    endpoint_url: Optional[str] = None
    manifest_cid: Optional[str] = None
    basename: Optional[str] = None
    framework: Optional[str] = None


class A2AMessage(BaseModel):
    from_agent_id: str
    to_agent_id: str
    message_type: str
    payload: dict[str, Any]
    reply_to: Optional[str] = None


class A2AMessageResponse(BaseModel):
    message_id: str
    status: str


class StoredMessage(BaseModel):
    id: str
    from_agent_id: str
    to_agent_id: str
    message_type: str
    payload: dict[str, Any]
    reply_to: Optional[str] = None
    status: str = ""
    created_at: Optional[str] = None


# ── x402 ─────────────────────────────────────────────────────────────

class PaymentRequirements(BaseModel):
    version: str
    scheme: str
    network: str
    asset: str
    pay_to: str = Field(alias="payTo")
    amount: str
    max_timeout_seconds: int = Field(alias="maxTimeoutSeconds")
    resource: str
    mime_type: str = Field(default="", alias="mimeType")
    extra: dict[str, Any] = Field(default_factory=dict)

    model_config = {"populate_by_name": True}


class X402Agent(BaseModel):
    agent_id: str
    name: str
    description: str
    x402_endpoints: list[Endpoint] = Field(default_factory=list)


# ── Wallet ───────────────────────────────────────────────────────────

class WalletInfo(BaseModel):
    agent_id: str
    wallet_address: str
    chain: str = "base"


# ── Health ───────────────────────────────────────────────────────────

class HealthStatus(BaseModel):
    agent_id: str
    status: str
    latency_ms: Optional[float] = None
    last_checked: Optional[str] = None


# ── Validation ───────────────────────────────────────────────────────

class EndpointCheckResult(BaseModel):
    endpoint: str
    reachable: bool
    status_code: Optional[int] = None
    error: Optional[str] = None


class ValidationResult(BaseModel):
    valid: bool
    errors: list[str] = Field(default_factory=list)
    warnings: list[str] = Field(default_factory=list)
    endpoint_results: list[EndpointCheckResult] = Field(default_factory=list)


# ── Attestation ──────────────────────────────────────────────────────

class Attestation(BaseModel):
    id: str
    agent_id: str
    attestation_uid: str
    schema_uid: str
    tx_hash: str
    attester: str
    revoked: Optional[bool] = None
    created_at: Optional[str] = None


# ── Status ───────────────────────────────────────────────────────────

class RegistryStatus(BaseModel):
    total_agents: int = 0
    online_agents: int = 0
    total_attestations: int = 0
    recent_registrations: list[RegistryEntry] = Field(default_factory=list)
