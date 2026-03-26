from __future__ import annotations

from typing import Any, Optional

import httpx

from beacon_sdk.models import (
    AgentsManifest,
    A2AMessage,
    A2AMessageResponse,
    Attestation,
    DiscoveredAgent,
    HealthStatus,
    PaymentRequirements,
    RegisterRequest,
    RegisterResponse,
    RegistryEntry,
    RegistryStatus,
    StoredMessage,
    ValidationResult,
    WalletInfo,
    X402Agent,
)


class BeaconError(Exception):
    """Raised when the Beacon API returns an error."""

    def __init__(self, message: str, status_code: int, body: str = ""):
        super().__init__(message)
        self.status_code = status_code
        self.body = body


class BeaconClient:
    """Async Python client for the Beacon agent registry protocol."""

    def __init__(
        self,
        base_url: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ):
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            timeout=timeout,
            headers=self._build_headers(),
        )

    def _build_headers(self) -> dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        return headers

    async def _request(
        self,
        method: str,
        path: str,
        json: Any = None,
        params: Optional[dict[str, Any]] = None,
    ) -> Any:
        # Filter out None params
        if params:
            params = {k: v for k, v in params.items() if v is not None}

        resp = await self._client.request(
            method, path, json=json, params=params
        )
        if resp.status_code >= 400:
            raise BeaconError(
                f"HTTP {resp.status_code}: {resp.reason_phrase}",
                resp.status_code,
                resp.text,
            )
        return resp.json()

    async def close(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "BeaconClient":
        return self

    async def __aexit__(self, *args: Any) -> None:
        await self.close()

    # ── Generation ──────────────────────────────────────────────────

    async def generate(
        self, github_url: str, provider: Optional[str] = None
    ) -> dict[str, Any]:
        """Scan a GitHub repo and generate an AGENTS.md manifest."""
        body: dict[str, Any] = {"github_url": github_url}
        if provider:
            body["provider"] = provider
        return await self._request("POST", "/api/generate", json=body)

    async def validate(self, content: str) -> ValidationResult:
        """Validate AGENTS.md content."""
        data = await self._request("POST", "/validate", json={"content": content})
        return ValidationResult(**data)

    # ── Registry ────────────────────────────────────────────────────

    async def register(self, req: RegisterRequest) -> RegisterResponse:
        """Register a new agent in the Beacon registry."""
        data = await self._request("POST", "/api/registry", json=req.model_dump())
        return RegisterResponse(**data)

    async def search(
        self,
        query: Optional[str] = None,
        owner: Optional[str] = None,
        framework: Optional[str] = None,
        limit: int = 20,
        offset: int = 0,
    ) -> list[RegistryEntry]:
        """Search the agent registry."""
        data = await self._request(
            "GET",
            "/api/registry",
            params={
                "query": query,
                "owner": owner,
                "framework": framework,
                "limit": limit,
                "offset": offset,
            },
        )
        return [RegistryEntry(**e) for e in data]

    async def get_agent(self, agent_id: str) -> RegistryEntry:
        """Get a single agent by ID."""
        data = await self._request("GET", f"/api/registry/{agent_id}")
        return RegistryEntry(**data)

    async def resolve_basename(self, name: str) -> dict[str, str]:
        """Resolve a Base ENS name to an address."""
        return await self._request("GET", f"/api/basenames/resolve/{name}")

    async def pin(self, agent_id: str) -> dict[str, str]:
        """Pin an agent's manifest to IPFS."""
        return await self._request("POST", f"/api/registry/{agent_id}/pin")

    # ── Wallets ─────────────────────────────────────────────────────

    async def get_wallet(self, agent_id: str) -> WalletInfo:
        """Get wallet info for an agent."""
        data = await self._request("GET", f"/api/registry/{agent_id}/wallet")
        return WalletInfo(**data)

    async def provision_wallet(
        self, agent_id: str, wallet_address: Optional[str] = None
    ) -> WalletInfo:
        """Provision or link a wallet to an agent."""
        body: dict[str, Any] = {}
        if wallet_address:
            body["wallet_address"] = wallet_address
        data = await self._request(
            "POST", f"/api/registry/{agent_id}/wallet", json=body
        )
        return WalletInfo(**data)

    # ── A2A Discovery & Messaging ───────────────────────────────────

    async def discover(
        self,
        capability: Optional[str] = None,
        framework: Optional[str] = None,
        limit: int = 20,
        offset: int = 0,
    ) -> list[DiscoveredAgent]:
        """Discover agents by capability or framework."""
        data = await self._request(
            "GET",
            "/api/a2a/discover",
            params={
                "capability": capability,
                "framework": framework,
                "limit": limit,
                "offset": offset,
            },
        )
        return [DiscoveredAgent(**a) for a in data]

    async def send_message(self, msg: A2AMessage) -> A2AMessageResponse:
        """Send a message to another agent."""
        data = await self._request("POST", "/api/a2a/messages", json=msg.model_dump())
        return A2AMessageResponse(**data)

    async def get_messages(
        self, agent_id: str, limit: int = 50
    ) -> list[StoredMessage]:
        """Get inbox messages for an agent."""
        data = await self._request(
            "GET", f"/api/a2a/messages/{agent_id}", params={"limit": limit}
        )
        return [StoredMessage(**m) for m in data]

    async def register_endpoint(
        self, agent_id: str, endpoint_url: str, owner_address: str
    ) -> None:
        """Register an A2A webhook endpoint for an agent."""
        await self._request(
            "POST",
            "/api/a2a/endpoint",
            json={
                "agent_id": agent_id,
                "endpoint_url": endpoint_url,
                "owner_address": owner_address,
            },
        )

    # ── Attestations ────────────────────────────────────────────────

    async def attest(
        self, agent_id: str, schema_uid: str, data: dict[str, Any]
    ) -> Attestation:
        """Create an EAS attestation for an agent."""
        resp = await self._request(
            "POST",
            f"/api/registry/{agent_id}/attest",
            json={"agent_id": agent_id, "schema_uid": schema_uid, "data": data},
        )
        return Attestation(**resp)

    async def get_attestations(self, agent_id: str) -> list[Attestation]:
        """Get attestations for an agent."""
        data = await self._request(
            "GET", f"/api/registry/{agent_id}/attestations"
        )
        return [Attestation(**a) for a in data]

    # ── x402 ────────────────────────────────────────────────────────

    async def get_x402_requirements(
        self, resource: Optional[str] = None
    ) -> PaymentRequirements:
        """Get x402 payment requirements for a resource."""
        data = await self._request(
            "GET", "/api/x402/requirements", params={"resource": resource}
        )
        return PaymentRequirements(**data)

    async def verify_x402(
        self,
        payment_payload: dict[str, Any],
        payment_requirements: dict[str, Any],
    ) -> dict[str, bool]:
        """Verify an x402 payment."""
        return await self._request(
            "POST",
            "/api/x402/verify",
            json={
                "paymentPayload": payment_payload,
                "paymentRequirements": payment_requirements,
            },
        )

    async def settle_x402(
        self,
        payment_payload: dict[str, Any],
        payment_requirements: dict[str, Any],
    ) -> dict[str, Any]:
        """Settle an x402 payment."""
        return await self._request(
            "POST",
            "/api/x402/settle",
            json={
                "paymentPayload": payment_payload,
                "paymentRequirements": payment_requirements,
            },
        )

    async def discover_x402(self) -> list[X402Agent]:
        """Discover agents with x402-enabled endpoints."""
        data = await self._request("GET", "/api/x402/discover")
        return [X402Agent(**a) for a in data]

    # ── Health ──────────────────────────────────────────────────────

    async def check_health(self, agent_id: str) -> HealthStatus:
        """Trigger a health check for an agent."""
        data = await self._request(
            "POST", f"/api/registry/{agent_id}/health"
        )
        return HealthStatus(**data)

    async def get_health(self, agent_id: str) -> HealthStatus:
        """Get health status of an agent."""
        data = await self._request(
            "GET", f"/api/registry/{agent_id}/health"
        )
        return HealthStatus(**data)

    async def list_health(
        self, status: Optional[str] = None, limit: int = 20
    ) -> list[HealthStatus]:
        """List all agent health statuses."""
        data = await self._request(
            "GET", "/api/health", params={"status": status, "limit": limit}
        )
        return [HealthStatus(**h) for h in data]

    # ── Tags ────────────────────────────────────────────────────────

    async def set_tags(self, agent_id: str, tags: list[str]) -> None:
        """Set tags for an agent."""
        await self._request(
            "PUT", f"/api/registry/{agent_id}/tags", json={"tags": tags}
        )

    async def get_tags(self, agent_id: str) -> list[str]:
        """Get tags for an agent."""
        return await self._request("GET", f"/api/registry/{agent_id}/tags")

    # ── Status ──────────────────────────────────────────────────────

    async def get_status(self) -> RegistryStatus:
        """Get registry status overview."""
        data = await self._request("GET", "/api/status")
        return RegistryStatus(**data)

    # ── Analytics ───────────────────────────────────────────────────

    async def get_stats(self, agent_id: str) -> dict[str, Any]:
        """Get agent analytics stats."""
        return await self._request("GET", f"/api/registry/{agent_id}/stats")

    async def get_trending(self, limit: int = 20) -> list[dict[str, Any]]:
        """Get trending agents."""
        return await self._request(
            "GET", "/api/analytics/trending", params={"limit": limit}
        )
