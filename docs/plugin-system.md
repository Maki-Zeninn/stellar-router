# Plugin System for Liquidity Sources

A modular interface for registering and querying liquidity providers via router-registry.

## Concept
Each liquidity provider is a separate Soroban contract that implements the
LiquidityPlugin interface. Plugins are registered in router-registry under a
well-known name (e.g. "liquidity/uniswap-v3") and resolved via router-core.

## Plugin Interface
Every plugin contract must expose:

  fn get_quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128
  fn execute_swap(env: Env, caller: Address, token_in: Address, token_out: Address,
                  amount_in: i128, min_amount_out: i128) -> i128
  fn name(env: Env) -> String
  fn version(env: Env) -> u32

## Registering a Plugin
stellar contract invoke --id <REGISTRY_ID> \
  -- register \
  --caller <ADMIN> \
  --name "liquidity/my-dex" \
  --address <PLUGIN_CONTRACT_ID> \
  --version 1

## Routing to a Plugin
stellar contract invoke --id <CORE_ID> \
  -- register_route \
  --caller <ADMIN> \
  --name "liquidity/my-dex" \
  --address <PLUGIN_CONTRACT_ID>

## Example Plugin (Rust skeleton)
#[contract]
pub struct MyDexPlugin;

#[contractimpl]
impl MyDexPlugin {
    pub fn get_quote(_env: Env, _token_in: Address, _token_out: Address, amount_in: i128) -> i128 {
        amount_in * 99 / 100  // 1% fee example
    }
    pub fn name(_env: Env) -> soroban_sdk::String { ... }
    pub fn version(_env: Env) -> u32 { 1 }
}
