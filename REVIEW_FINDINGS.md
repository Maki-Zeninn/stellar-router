# Review findings: duplicate-symbol reports (no change needed)

A series of automated/reported findings claimed duplicate symbols in three
places. This document records the verification results as of commit
`0f17747` on `main`.

## 1. `router-execution::DataKey::ExecHistory` / `MaxHistorySize` duplicate

**Reported:** `contracts/router-execution/src/lib.rs` lines 50–52 contain a
duplicate `ExecHistory` variant.

**Status:** Real historically, already fixed. The duplicate `MaxHistorySize`
variant existed prior to commit `0557dd8` ("fix: repair broken CI pipeline
and clean up bad merges across contracts") and was removed there. Current
`DataKey` enum (lib.rs:59–68) has exactly one `ExecHistory` and one
`MaxHistorySize` variant. No further action needed.

## 2. `api-server` duplicate `use tracing::info;` import

**Reported:** `api-server/src/main.rs` lines 20–21 import `tracing::info`
twice.

**Status:** Real historically, already fixed. Also resolved in `0557dd8`,
which collapsed `use tracing::info;` + `use tracing::{info, warn};` into the
single `use tracing::{info, info_span, warn, Instrument};` import currently
at line 23. No further action needed.

## 3. `router-timelock::DataKey` duplicate `Op(Bytes)` / `PendingOps`, and
   duplicate `initialize` signatures

**Reported:** `contracts/router-timelock/src/lib.rs` lines 25–33 and 99–113
contain duplicate `DataKey` variants and two overlapping `initialize`
function signatures.

**Status:** Not found in the codebase or its git history. `git log -p` for
`contracts/router-timelock/src/lib.rs` shows no commit ever introducing a
second `Op(Bytes)`, a second `PendingOps`, or a second `initialize` function
— only whitespace/formatting changes to that region. The current file has a
single `DataKey` enum with one `Op(Bytes)` and one `PendingOps` variant, and
a single `initialize(env, admin, min_delay, max_pending_ops)` function.

## Conclusion

No code changes are required. Findings 1 and 2 describe a real bug that was
already fixed on `main`; finding 3 does not correspond to any state the
repository has ever been in. This PR contains no functional changes — it
exists only to record the verification result.
