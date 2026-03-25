from beacon_sdk.client import BeaconClient
from beacon_sdk.models import (
    AgentsManifest,
    Capability,
    Endpoint,
    RegistryEntry,
    RegisterRequest,
    RegisterResponse,
    DiscoveredAgent,
    A2AMessage,
    A2AMessageResponse,
    WalletInfo,
    PaymentRequirements,
    ValidationResult,
    HealthStatus,
)

__version__ = "0.1.0"

__all__ = [
    "BeaconClient",
    "AgentsManifest",
    "Capability",
    "Endpoint",
    "RegistryEntry",
    "RegisterRequest",
    "RegisterResponse",
    "DiscoveredAgent",
    "A2AMessage",
    "A2AMessageResponse",
    "WalletInfo",
    "PaymentRequirements",
    "ValidationResult",
    "HealthStatus",
]
