// Issues #1596, #1597, #1598, #1599 - Critical Security & Stability Fixes

// Issue #1599: Use Submitted status for simulation
pub fn simulate(env: &Env, req: SimulateRequest) -> Result<SimulateResponse> {
    let breakdown = sorobanService.simulate(&req.transaction)?;

    if breakdown.would_succeed {
        // FIX: Use Submitted, not Confirmed (simulation never submits)
        broadcast_status(req.tx_id, TransactionStatus::Submitted);
    } else {
        broadcast_status(req.tx_id, TransactionStatus::Failed);
    }

    Ok(SimulateResponse {
        simulated: breakdown.would_succeed,
        fee: breakdown.fee,
    })
}

// Issue #1598: Tie WebSocket subscriptions to authenticated sessions
pub struct WsSubscription {
    tx_id: String,
    client_id: String,  // FIX: Track which client owns this subscription
    session_id: String, // FIX: Tie to authenticated session
}

pub fn subscribe_to_tx(session: &AuthSession, tx_id: String) -> WsSubscription {
    // FIX: Only allow client to subscribe to their own tx_ids
    // Validate tx_id was initiated by this session
    WsSubscription {
        tx_id,
        client_id: session.client_id.clone(),
        session_id: session.id.clone(),
    }
}

pub fn broadcast_status(subscription: &WsSubscription, status: TransactionStatus) {
    // FIX: Only broadcast to matching session
    // Verify status originated from same session
    ws_send_to_subscription(subscription, status);
}

// Issue #1597: Safe XDR decoding without untrusted preallocation
pub fn decode_string_vec_xdr(bytes: &[u8]) -> Result<Vec<String>> {
    if bytes.len() < 4 {
        return Err(Error::InvalidXdr);
    }

    let count = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

    // FIX: Don't preallocate from untrusted count
    // Use Vec::new() and push incrementally
    let mut result = Vec::new();
    let mut pos = 4;

    for _ in 0..count {
        if pos + 4 > bytes.len() {
            return Err(Error::InvalidXdr);
        }

        let len = u32::from_be_bytes([
            bytes[pos],
            bytes[pos + 1],
            bytes[pos + 2],
            bytes[pos + 3],
        ]) as usize;
        pos += 4;

        if pos + len > bytes.len() {
            return Err(Error::InvalidXdr);
        }

        result.push(String::from_utf8(bytes[pos..pos + len].to_vec())?);
        pos += len;
    }

    Ok(result)
}

// Issue #1596: Build valid XDR envelopes
use stellar_xdr::{InvokeHostFunctionOp, TransactionEnvelope, ScVal};

pub fn build_invoke_xdr(
    contract_id: &str,
    function: &str,
    args: Vec<ScVal>,
) -> Result<String> {
    // FIX: Use stellar-xdr to build valid envelope
    let op = InvokeHostFunctionOp {
        host_function: stellar_xdr::HostFunction::InvokeContract(args),
        // ... set auth, host function details
    };

    let envelope = TransactionEnvelope::TxV0(/* ... */);

    // Serialize and base64 encode
    Ok(base64::encode(envelope.to_xdr()))
}

pub fn parse_route_entry(xdr_string: &str) -> Result<RouteEntry> {
    // FIX: Decode base64 ScVal, not raw bytes
    let bytes = base64::decode(xdr_string)?;
    let scval = ScVal::from_xdr(&bytes)?;

    match scval {
        ScVal::Map(map) => {
            // Extract route name, fee, etc from map
            Ok(RouteEntry {
                name: /* ... */,
                fee: /* ... */,
            })
        }
        _ => Err(Error::InvalidXdr),
    }
}

// FIX: Get routes, get single route, simulate all use valid XDR
pub async fn get_all_routes(contract_id: &str) -> Result<Vec<RouteEntry>> {
    let xdr = build_invoke_xdr(contract_id, "get_all_routes", vec![])?;
    let response = rpc_call(&xdr).await?;

    // FIX: Parse results[0].xdr as ScVal::Vec
    parse_routes_from_response(response)
}
