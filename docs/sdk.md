# JavaScript / TypeScript SDK

A client library for interacting with the stellar-router contracts from JS/TS.

## Installation
npm install stellar-router-sdk

## Quick Start
import { RouterClient } from "stellar-router-sdk";

const client = new RouterClient({
  network: "testnet",
  coreContractId: "C...",
  keypair: Keypair.fromSecret("S..."),
});

const address = await client.resolve("oracle");
await client.registerRoute("oracle", "C...", { description: "Price feed" });

## API
- resolve(name) → Promise<string>
- registerRoute(name, address, metadata?) → Promise<void>
- updateRoute(name, newAddress) → Promise<void>
- removeRoute(name) → Promise<void>
- setRoutePaused(name, paused) → Promise<void>
- setPaused(paused) → Promise<void>
- getRoute(name) → Promise<RouteEntry | null>
- totalRouted() → Promise<bigint>

## Error Handling
All methods throw RouterSdkError with a .code (e.g. "RouteNotFound") on failure.

## Publishing
npm version patch && npm publish
