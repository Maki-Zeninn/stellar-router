# JavaScript / TypeScript SDK Proposal

This repository contains Soroban contracts and Rust services. It does **not**
currently publish an npm package named `stellar-router-sdk`, and this document
must not be read as an installation or support commitment.

## Proposed client surface

An eventual SDK could provide a typed client around the deployed contract
interfaces, for example:

```typescript
interface RouterClientProposal {
  resolve(name: string): Promise<string>
  registerRoute(name: string, address: string, metadata?: unknown): Promise<void>
  updateRoute(name: string, newAddress: string): Promise<void>
  removeRoute(name: string): Promise<void>
}
```

The exact package name, API, signing abstraction, network configuration, and
release process remain undecided. The interface above is illustrative design
material only; it is not generated from a published package and is not an API
that this repository currently guarantees.

## Current integration options

For JavaScript or TypeScript applications today, use the official Stellar SDK
or the Soroban RPC APIs directly, construct calls from the deployed contract
interfaces, and provide a wallet-specific signing flow. The contract entry
points and their Rust types in this repository are the source of truth until an
SDK is implemented and versioned.

Do not run `npm install stellar-router-sdk` based on this document. When an SDK
is implemented, it should first be added to this repository (or a clearly
identified companion repository), published under a confirmed package name,
and documented with a release version and support policy.
