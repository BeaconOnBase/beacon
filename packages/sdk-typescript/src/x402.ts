import type { BeaconClient } from "./client";
import type { PaymentRequirements, X402Agent } from "./types";

export class X402Client {
  constructor(
    private baseUrl: string,
    private client: BeaconClient
  ) {}

  /** Get x402 payment requirements for a resource */
  async getRequirements(resource?: string): Promise<PaymentRequirements> {
    return this.client.request<PaymentRequirements>(
      "GET",
      "/api/x402/requirements",
      undefined,
      { resource }
    );
  }

  /** Verify an x402 payment */
  async verify(
    paymentPayload: Record<string, unknown>,
    paymentRequirements: PaymentRequirements
  ): Promise<{ success: boolean }> {
    return this.client.request<{ success: boolean }>(
      "POST",
      "/api/x402/verify",
      { paymentPayload, paymentRequirements }
    );
  }

  /** Settle an x402 payment */
  async settle(
    paymentPayload: Record<string, unknown>,
    paymentRequirements: PaymentRequirements
  ): Promise<{
    success: boolean;
    transaction?: string;
    network?: string;
    payer?: string;
  }> {
    return this.client.request("POST", "/api/x402/settle", {
      paymentPayload,
      paymentRequirements,
    });
  }

  /** Discover agents with x402-enabled endpoints */
  async discover(): Promise<X402Agent[]> {
    return this.client.request<X402Agent[]>("GET", "/api/x402/discover");
  }
}
