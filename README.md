# Beacon

![Language](https://img.shields.io/badge/built%20with-Rust-orange?style=flat-square&logo=rust)
![Tests](https://img.shields.io/github/actions/workflow/status/BeaconOnBase/beacon/release.yml?label=tests&style=flat-square)
![Version](https://img.shields.io/badge/version-0.4.2-blue?style=flat-square)
![License](https://img.shields.io/badge/license-BUSL--1.1-blue?style=flat-square)

**The Verifiable Agentic Protocol.** Make any repository agent-ready. Instantly.

Beacon is a protocol designed for the Web 4.0 agentic economy. It scans your codebase, infers its capabilities using AI, registers a unique on-chain identity, and exposes standardized tools for autonomous agents via the Model Context Protocol (MCP).

---

## Core Features

- **Verifiable Generation (ZK):** Cryptographically prove that `AGENTS.md` matches a specific Git state via SP1 Zero-Knowledge proofs.
- **A2A Discovery Protocol:** Full support for the Google Agent-to-Agent standard with enhanced `agent-card.json` export and JSON-RPC messaging.
- **AI-Powered Inference:** Automatically generate AAIF-compliant [AGENTS.md](https://github.com/agentmd/agent.md) manifests with 10+ AI provider options.
- **On-Chain Identity:** Register your repository's provenance on Base Mainnet via ERC-7527.
- **Native MCP Server:** Standards-compliant tool discovery for LLMs (Claude, Cursor, etc.).
- **Official SDKs:** Python and TypeScript SDKs covering all 30+ REST endpoints.
- **x402 Payment Protocol:** Integrated x402 support for pay-per-run verification on Base/Solana.

---

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/BeaconOnBase/beacon/master/install.sh | sh
```

---

## Quickstart

**1. Generate Verifiable Manifest**
```bash
export GEMINI_API_KEY=your_key
beacon generate ./my-project --provider gemini
# Produces AGENTS.md with AI-inferred capabilities
```

**2. Register On-Chain Identity**
```bash
export AGENT_PRIVATE_KEY=0x...
beacon register ./ --chain base
```

**3. Serve as MCP Tool**
```bash
beacon serve --port 8080
# AI agents can now connect to http://localhost:8080/sse
```

**4. Discover Other Agents (A2A)**
```bash
# Via API
curl http://localhost:8080/api/a2a/discover?capability=token_swap
```

---

## Usage

### Protocol Commands

| Command | Description |
|---|---|
| `generate` | Scans a repo and creates an AGENTS.md manifest with AI-inferred capabilities. |
| `register` | Mints an ERC-7527 identity NFT for the repo on Base. |
| `validate` | Checks an AGENTS.md for standards compliance (with optional endpoint reachability check). |
| `serve` | Starts a dual-protocol (REST + MCP) server with 30+ API endpoints. |
| `upgrade` | Automatically upgrades Beacon CLI to the latest version. |
| `farcaster-bot` | Runs the Farcaster bot for monitoring agent mentions and scanning requests. |

### REST API Endpoints

The `beacon serve` command exposes the following endpoints:

| Endpoint | Method | Description |
|---|---|---|
| `/api/generate` | POST | Generate AGENTS.md from repo context |
| `/api/scan-generate` | POST | Scan GitHub URL and generate manifest |
| `/api/validate` | POST | Validate AGENTS.md content |
| `/api/registry` | GET/POST | List agents or register new agent |
| `/api/registry/{id}` | GET/PUT/DELETE | Get, update, or delete agent |
| `/api/registry/{id}/export` | GET | Export agent card (JSON-LD or A2A format) |
| `/api/a2a/discover` | GET | Discover agents by capability |
| `/api/a2a/send` | POST | Send message to agent |
| `/api/a2a/messages/{id}` | GET | Get agent inbox |
| `/api/a2a/endpoints` | POST | Register webhook endpoint |
| `/api/health` | GET | Health check with agent status |
| `/api/analytics` | GET | Analytics and usage stats |
| `/api/tags` | GET/POST | Manage agent tags |
| `/api/status` | GET | Registry status overview |
| `/api/x402/verify` | POST | Verify x402 payment |
| `/sse` | GET | MCP Server-Sent Events stream |

### AI Providers
| Provider | --provider flag | Key |
|---|---|---|
| Gemini 2.5 Flash | `gemini` (default) | `GEMINI_API_KEY` |
| Claude 3.5/3.7 | `claude` | `CLAUDE_API_KEY` |
| OpenAI GPT-4o | `openai` | `OPENAI_API_KEY` |
| DeepSeek V3 | `deepseek` | `DEEPSEEK_API_KEY` |
| Qwen 2.5 Max | `qwen` | `DASHSCOPE_API_KEY` |
| Grok 2 | `grok` | `XAI_API_KEY` |
| Llama 3 | `llama` | `LLAMA_API_KEY` |
| Mistral Large | `mistral` | `MISTRAL_API_KEY` |
| ZAI (GLM-4.5) | `zai` | `ZAI_API_KEY` |
| Beacon Cloud | `beacon-ai-cloud` | none — $0.09/run via USDC |

---

## SDKs

Beacon provides official SDKs for Python and TypeScript, covering all 30+ REST API endpoints.

### Python SDK

```bash
pip install beacon-sdk
```

```python
from beacon_sdk import BeaconClient
from beacon_sdk.models import RegisterRequest, A2AMessage

client = BeaconClient(base_url="https://api.beaconcloud.org")

# Generate AGENTS.md
manifest = await client.generate("./my-project", provider="gemini")

# Register on-chain identity
entry = await client.registry.register(RegisterRequest(
    repo_path="./",
    chain="base"
))

# A2A messaging
response = await client.a2a.send_message(A2AMessage(
    from_agent_id="agent-1",
    to_agent_id="agent-2",
    message_type="handshake",
    payload={"hello": "world"}
))

# x402 payment verification
payment = await client.x402.verify_payment(
    chain="base",
    txn_hash="0x...",
    amount=0.09
)
```

### TypeScript SDK

```bash
npm install @beacon-protocol/sdk
```

```typescript
import { BeaconClient } from '@beacon-protocol/sdk';
import type { RegisterRequest, A2AMessage } from '@beacon-protocol/sdk';

const client = new BeaconClient({ baseURL: 'https://api.beaconcloud.org' });

// Generate AGENTS.md
const manifest = await client.generate('./my-project', { provider: 'gemini' });

// Register on-chain identity
const entry = await client.registry.register({
  repoPath: './',
  chain: 'base'
} as RegisterRequest);

// A2A messaging
const response = await client.a2a.sendMessage({
  fromAgentId: 'agent-1',
  toAgentId: 'agent-2',
  messageType: 'handshake',
  payload: { hello: 'world' }
} as A2AMessage);

// x402 payment verification
const payment = await client.x402.verifyPayment({
  chain: 'base',
  txnHash: '0x...',
  amount: 0.09
});
```

---

## A2A Agent Discovery

Beacon implements the full Google A2A (Agent-to-Agent) Discovery Protocol:

- **Agent Card Export:** Generate standardized `agent-card.json` files for registered agents (JSON-LD or A2A format)
- **Capability Discovery:** Find agents by capability, framework, or attestation status  
- **Secure Messaging:** JSON-RPC based agent-to-agent communication with webhook delivery
- **Health Monitoring:** Real-time health status and attestation tracking

**API Endpoints:**
```bash
# Export agent card (JSON-LD or A2A format)
GET /api/registry/{id}/export?format=a2a

# Discover agents by capability
GET /api/a2a/discover?capability=token_swap&framework=OpenClaw

# Send message to another agent
POST /api/a2a/send
{
  "from_agent_id": "agent-1",
  "to_agent_id": "agent-2",
  "message_type": "handshake",
  "payload": {"hello": "world"}
}

# Get agent inbox
GET /api/a2a/messages/{agent_id}?limit=20
```

---

## x402 Payment Protocol

Beacon integrates the x402 payment protocol for monetized API access:

- **Pay-Per-Run:** $0.09 USDC per AGENTS.md generation via Beacon Cloud
- **Multi-Chain:** Support for Base and Solana USDC payments
- **Automatic Verification:** On-chain transaction verification with replay protection
- **SDK Integration:** x402 client available in Python and TypeScript SDKs

**Payment Headers:**
```
x-payment-txn-hash: 0x...
x-payment-chain: base
x-payment-run-id: uuid-here
```

**Verification Flow:**
1. Client initiates generation request without API key
2. Beacon returns payment requirements (run ID, wallet address, amount)
3. Client sends USDC payment to provided address
4. Client resubmits request with payment headers
5. Beacon verifies on-chain transaction and processes request

---

## Interoperability (MCP)

Beacon implements the Model Context Protocol. Any agent that speaks MCP can discover Beacon tools automatically.

**Claude Desktop Configuration:**
```json
{
  "mcpServers": {
    "beacon": {
      "url": "https://api.beaconcloud.org/sse"
    }
  }
}
```

---

## How it works

1. **Scan**: Walks the repo, extracting source files, package manifests, and OpenAPI specs.
2. **Infer**: Identifies capabilities, endpoints, and schemas using framework-aware AI inference.
3. **Register**: Wraps the repository URL into a unique on-chain identity token (ERC-7527).
4. **Expose**: Serves tools to the agentic web via standard REST and MCP interfaces.
