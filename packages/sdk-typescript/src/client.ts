import { RegistryClient } from "./registry";
import { A2AClient } from "./a2a";
import { X402Client } from "./x402";
import type {
  GenerateRequest,
  GenerateResponse,
  ValidateRequest,
  ValidationResult,
  HealthStatus,
  RegistryStatus,
} from "./types";

export interface BeaconClientOptions {
  baseUrl: string;
  apiKey?: string;
  timeout?: number;
}

export class BeaconClient {
  private baseUrl: string;
  private apiKey?: string;
  private timeout: number;

  public readonly registry: RegistryClient;
  public readonly a2a: A2AClient;
  public readonly x402: X402Client;

  constructor(options: BeaconClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.apiKey = options.apiKey;
    this.timeout = options.timeout ?? 30000;

    this.registry = new RegistryClient(this.baseUrl, this);
    this.a2a = new A2AClient(this.baseUrl, this);
    this.x402 = new X402Client(this.baseUrl, this);
  }

  /** @internal */
  async request<T>(
    method: string,
    path: string,
    body?: unknown,
    query?: Record<string, string | number | boolean | undefined>
  ): Promise<T> {
    const url = new URL(`${this.baseUrl}${path}`);
    if (query) {
      for (const [key, value] of Object.entries(query)) {
        if (value !== undefined) {
          url.searchParams.set(key, String(value));
        }
      }
    }

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    if (this.apiKey) {
      headers["Authorization"] = `Bearer ${this.apiKey}`;
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await fetch(url.toString(), {
        method,
        headers,
        body: body ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      if (!response.ok) {
        const errorBody = await response.text().catch(() => "");
        throw new BeaconError(
          `HTTP ${response.status}: ${response.statusText}`,
          response.status,
          errorBody
        );
      }

      return (await response.json()) as T;
    } finally {
      clearTimeout(timeoutId);
    }
  }

  // ── Generation ──────────────────────────────────────────────────

  /** Scan a GitHub repo and generate an AGENTS.md manifest */
  async generate(req: GenerateRequest): Promise<GenerateResponse> {
    return this.request<GenerateResponse>("POST", "/api/generate", req);
  }

  /** Validate AGENTS.md content */
  async validate(req: ValidateRequest): Promise<ValidationResult> {
    return this.request<ValidationResult>("POST", "/validate", req);
  }

  // ── Health ──────────────────────────────────────────────────────

  /** Check health of a specific agent */
  async checkHealth(agentId: string): Promise<HealthStatus> {
    return this.request<HealthStatus>("POST", `/api/registry/${agentId}/health`);
  }

  /** Get health status of a specific agent */
  async getHealth(agentId: string): Promise<HealthStatus> {
    return this.request<HealthStatus>("GET", `/api/registry/${agentId}/health`);
  }

  /** List all agent health statuses */
  async listHealth(status?: string, limit?: number): Promise<HealthStatus[]> {
    return this.request<HealthStatus[]>("GET", "/api/health", undefined, {
      status,
      limit,
    });
  }

  // ── Status ──────────────────────────────────────────────────────

  /** Get registry status overview */
  async getStatus(): Promise<RegistryStatus> {
    return this.request<RegistryStatus>("GET", "/api/status");
  }
}

export class BeaconError extends Error {
  public readonly statusCode: number;
  public readonly body: string;

  constructor(message: string, statusCode: number, body: string) {
    super(message);
    this.name = "BeaconError";
    this.statusCode = statusCode;
    this.body = body;
  }
}
