## Technical Overview

### Issue Analysis
In `api-server/src/rpc.rs`, magic numbers (`100`, `8_000`, `200u32`, `100u32`) were duplicated across both `simulate()` and `heuristic_estimate()` for fee and surge multiplier calculations. Duplicating fee calculation constants creates maintenance risks if parameters need updating in the future.

### Refactoring Strategy
1. **Module-Level Constant Definitions**: Define clear, idiomatic Rust module-level `const` items for fee estimation:
   - `BASE_FEE`: The default base transaction fee (`i64 = 100`).
   - `HIGH_LOAD_THRESHOLD_BPS`: Network load threshold in basis points (`u32 = 8_000`).
   - `HIGH_LOAD_SURGE_MULTIPLIER`: Multiplier applied during high network load (`u32 = 200`).
   - `NORMAL_SURGE_MULTIPLIER`: Default multiplier during normal network load (`u32 = 100`).

2. **Function Updates**: Replace literal magic values in both `simulate()` and `heuristic_estimate()` with references to these shared constants.

---

## Patch Solution

```patch
--- a/api-server/src/rpc.rs
+++ b/api-server/src/rpc.rs
@@ -10,6 +10,12 @@
 // File: api-server/src/rpc.rs
 
+// Fee and Surge Multiplier Constants
+const BASE_FEE: i64 = 100;
+const HIGH_LOAD_THRESHOLD_BPS: u32 = 8_000;
+const HIGH_LOAD_SURGE_MULTIPLIER: u32 = 200;
+const NORMAL_SURGE_MULTIPLIER: u32 = 100;
+
 pub impl RpcServer {
     pub fn simulate(&self, ...) -> Result<SimulationResult, RpcError> {
-        let base_fee: i64 = 100;
-        let (surge_multiplier, high_load) = if network_load_bps >= 8_000 {
-            (200u32, true)
-        } else {
-            (100u32, false)
-        };
+        let base_fee: i64 = BASE_FEE;
+        let (surge_multiplier, high_load) = if network_load_bps >= HIGH_LOAD_THRESHOLD_BPS {
+            (HIGH_LOAD_SURGE_MULTIPLIER, true)
+        } else {
+            (NORMAL_SURGE_MULTIPLIER, false)
+        };
         ...
     }
 
     pub fn heuristic_estimate(&self, ...) -> Result<FeeEstimate, RpcError> {
-        let base_fee: i64 = 100;
-        let (surge_multiplier, high_load) = if network_load_bps >= 8_000 {
-            (200u32, true)
-        } else {
-            (100u32, false)
-        };
+        let base_fee: i64 = BASE_FEE;
+        let (surge_multiplier, high_load) = if network_load_bps >= HIGH_LOAD_THRESHOLD_BPS {
+            (HIGH_LOAD_SURGE_MULTIPLIER, true)
+        } else {
+            (NORMAL_SURGE_MULTIPLIER, false)
+        };
         ...
     }
 }
```

---

## Complete Rust Code Refactor (`api-server/src/rpc.rs` snippet)

```rust
// Module-level named constants for RPC fee estimation
pub const BASE_FEE: i64 = 100;
pub const HIGH_LOAD_THRESHOLD_BPS: u32 = 8_000;
pub const HIGH_LOAD_SURGE_MULTIPLIER: u32 = 200;
pub const NORMAL_SURGE_MULTIPLIER: u32 = 100;

/// Helper to compute network surge multiplier and high load state
#[inline]
pub fn calculate_surge_multiplier(network_load_bps: u32) -> (u32, bool) {
    if network_load_bps >= HIGH_LOAD_THRESHOLD_BPS {
        (HIGH_LOAD_SURGE_MULTIPLIER, true)
    } else {
        (NORMAL_SURGE_MULTIPLIER, false)
    }
}

// In RPC methods:
pub fn simulate(&self, network_load_bps: u32) -> i64 {
    let base_fee: i64 = BASE_FEE;
    let (surge_multiplier, high_load) = calculate_surge_multiplier(network_load_bps);
    
    (base_fee * surge_multiplier as i64) / 100
}

pub fn heuristic_estimate(&self, network_load_bps: u32) -> i64 {
    let base_fee: i64 = BASE_FEE;
    let (surge_multiplier, high_load) = calculate_surge_multiplier(network_load_bps);
    
    (base_fee * surge_multiplier as i64) / 100
}
```