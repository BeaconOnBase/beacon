import type { BeaconClient } from "./client";
import type {
  DiscoveryQuery,
  DiscoveredAgent,
  A2AMessage,
  A2AMessageResponse,
  StoredMessage,
} from "./types";

export class A2AClient {
  constructor(
    private baseUrl: string,
    private client: BeaconClient
  ) {}

  /** Discover agents by capability or framework */
  async discover(query?: DiscoveryQuery): Promise<DiscoveredAgent[]> {
    return this.client.request<DiscoveredAgent[]>(
      "GET",
      "/api/a2a/discover",
      undefined,
      query as Record<string, string | number | boolean | undefined>
    );
  }

  /** Send a message to another agent */
  async sendMessage(msg: A2AMessage): Promise<A2AMessageResponse> {
    return this.client.request<A2AMessageResponse>(
      "POST",
      "/api/a2a/messages",
      msg
    );
  }

  /** Get inbox messages for an agent */
  async getMessages(
    agentId: string,
    limit?: number
  ): Promise<StoredMessage[]> {
    return this.client.request<StoredMessage[]>(
      "GET",
      `/api/a2a/messages/${agentId}`,
      undefined,
      { limit }
    );
  }

  /** Register an A2A webhook endpoint for an agent */
  async registerEndpoint(
    agentId: string,
    endpointUrl: string,
    ownerAddress: string
  ): Promise<void> {
    await this.client.request<void>("POST", "/api/a2a/endpoint", {
      agent_id: agentId,
      endpoint_url: endpointUrl,
      owner_address: ownerAddress,
    });
  }
}
