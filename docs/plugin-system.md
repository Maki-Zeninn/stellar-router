# Plugin System for Liquidity Sources

A proposed convention for third-party liquidity-provider contracts.

> **Current status:** The router contracts in this repository do not currently
> call `get_quote` or `execute_swap` on registered plugins. `router-registry`
> stores addresses and versions, while `router-core` resolves routes; neither
> contract dispatches the interface below. Integrators must perform any plugin
> invocation explicitly in their own application or contract.

## Concept
An ecosystem liquidity provider may choose to implement the following
`LiquidityPlugin` interface. A provider can be registered in `router-registry`
under a well-known name (for example, `liquidity/my-dex`) and resolved through
the registry/core address records, but registration does not make the router
invoke the provider automatically.

## Plugin Interface

An implementing plugin should expose:

```rust
fn get_quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128
fn execute_swap(env: Env, caller: Address, token_in: Address, token_out: Address,
                amount_in: i128, min_amount_out: i128) -> i128
fn name(env: Env) -> String
fn version(env: Env) -> u32
```

## Registering a Plugin

```bash
stellar contract invoke --id <REGISTRY_ID> \
  -- register \
  --caller <ADMIN> \
  --name "liquidity/my-dex" \
  --address <PLUGIN_CONTRACT_ID> \
  --version 1
```

## Resolving a Plugin Address

```bash
stellar contract invoke --id <CORE_ID> \
  -- register_route \
  --caller <ADMIN> \
  --name "liquidity/my-dex" \
  --address <PLUGIN_CONTRACT_ID>
```

## Example Third-Party Contract (Rust skeleton)

```rust
#[contract]
pub struct MyDexPlugin;

#[contractimpl]
impl MyDexPlugin {
    pub fn get_quote(_env: Env, _token_in: Address, _token_out: Address, amount_in: i128) -> i128 {
        amount_in * 99 / 100  // 1% fee example
    }
    pub fn name(_env: Env) -> soroban_sdk::String { todo!() }
    pub fn version(_env: Env) -> u32 { 1 }
}
```
