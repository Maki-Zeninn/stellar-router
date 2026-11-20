# Transaction Signing Abstraction Proposal

This document describes a possible client-side abstraction for a future SDK.
The repository currently contains no JavaScript/TypeScript signer
implementation and does not publish `stellar-router-sdk`.

## Proposed signer interface (TypeScript)

```typescript
interface Signer {
  publicKey(): string;
  sign(transaction: Transaction): Promise<Transaction>;
}
```

## Planned implementations

An eventual client library may provide adapters for local key material,
hardware wallets, and browser wallets such as Freighter or WalletConnect. No
such adapters are included in this repository today. Applications should use
the official Stellar SDK and their wallet provider's documented signing API
until a versioned SDK is implemented.

## Future custom signers

The intended extension point is to implement the proposed `Signer` interface
and pass it to a future router client. The transaction type, network handling,
error model, and package boundary must be specified before this becomes a
supported API.
