import type { BeaconClient } from "./client";
import type {
  RegistryEntry,
  RegisterRequest,
  RegisterResponse,
  RegistryQuery,
  WalletInfo,
  Attestation,
  CreateAttestationRequest,
} from "./types";

export class RegistryClient {
  constructor(
    private baseUrl: string,
    private client: BeaconClient
  ) {}

  /** Register a new agent in the Beacon registry */
  async register(req: RegisterRequest): Promise<RegisterResponse> {
    return this.client.request<RegisterResponse>("POST", "/api/registry", req);
  }

  /** Search the agent registry */
  async search(query?: RegistryQuery): Promise<RegistryEntry[]> {
    return this.client.request<RegistryEntry[]>(
      "GET",
      "/api/registry",
      undefined,
      query as Record<string, string | number | boolean | undefined>
    );
  }

  /** Get a single agent by ID */
  async getAgent(agentId: string): Promise<RegistryEntry> {
    return this.client.request<RegistryEntry>(
      "GET",
      `/api/registry/${agentId}`
    );
  }

  /** Resolve a Base ENS name to an address */
  async resolveBasename(
    name: string
  ): Promise<{ name: string; address: string }> {
    return this.client.request<{ name: string; address: string }>(
      "GET",
      `/api/basenames/resolve/${name}`
    );
  }

  /** Pin an agent's manifest to IPFS */
  async pin(agentId: string): Promise<{ cid: string }> {
    return this.client.request<{ cid: string }>(
      "POST",
      `/api/registry/${agentId}/pin`
    );
  }

  // ── Wallets ───────────────────────────────────────────────────

  /** Get wallet info for an agent */
  async getWallet(agentId: string): Promise<WalletInfo> {
    return this.client.request<WalletInfo>(
      "GET",
      `/api/registry/${agentId}/wallet`
    );
  }

  /** Provision or link a wallet to an agent */
  async provisionWallet(
    agentId: string,
    walletAddress?: string
  ): Promise<WalletInfo> {
    return this.client.request<WalletInfo>(
      "POST",
      `/api/registry/${agentId}/wallet`,
      walletAddress ? { wallet_address: walletAddress } : {}
    );
  }

  // ── Attestations ──────────────────────────────────────────────

  /** Create an EAS attestation for an agent */
  async attest(
    agentId: string,
    req: CreateAttestationRequest
  ): Promise<Attestation> {
    return this.client.request<Attestation>(
      "POST",
      `/api/registry/${agentId}/attest`,
      req
    );
  }

  /** Get attestations for an agent */
  async getAttestations(agentId: string): Promise<Attestation[]> {
    return this.client.request<Attestation[]>(
      "GET",
      `/api/registry/${agentId}/attestations`
    );
  }

  /** Get a specific attestation by UID */
  async getAttestation(uid: string): Promise<Attestation> {
    return this.client.request<Attestation>(
      "GET",
      `/api/attestations/${uid}`
    );
  }

  // ── Tags ──────────────────────────────────────────────────────

  /** Set tags for an agent */
  async setTags(agentId: string, tags: string[]): Promise<void> {
    await this.client.request<void>(
      "PUT",
      `/api/registry/${agentId}/tags`,
      { tags }
    );
  }

  /** Get tags for an agent */
  async getTags(agentId: string): Promise<string[]> {
    return this.client.request<string[]>(
      "GET",
      `/api/registry/${agentId}/tags`
    );
  }

  /** Search agents by tag */
  async searchByTag(
    tag: string,
    limit?: number,
    offset?: number
  ): Promise<string[]> {
    return this.client.request<string[]>(
      "GET",
      "/api/tags/search",
      undefined,
      { tag, limit, offset }
    );
  }

  /** Get popular tags */
  async getPopularTags(
    limit?: number
  ): Promise<{ tag: string; count: number }[]> {
    return this.client.request<{ tag: string; count: number }[]>(
      "GET",
      "/api/tags/popular",
      undefined,
      { limit }
    );
  }

  // ── Analytics ─────────────────────────────────────────────────

  /** Get agent analytics stats */
  async getStats(
    agentId: string
  ): Promise<Record<string, unknown>> {
    return this.client.request<Record<string, unknown>>(
      "GET",
      `/api/registry/${agentId}/stats`
    );
  }

  /** Get agent event log */
  async getEvents(
    agentId: string,
    limit?: number,
    offset?: number
  ): Promise<Record<string, unknown>[]> {
    return this.client.request<Record<string, unknown>[]>(
      "GET",
      `/api/registry/${agentId}/events`,
      undefined,
      { limit, offset }
    );
  }

  /** Get trending agents */
  async getTrending(
    limit?: number
  ): Promise<Record<string, unknown>[]> {
    return this.client.request<Record<string, unknown>[]>(
      "GET",
      "/api/analytics/trending",
      undefined,
      { limit }
    );
  }
}
