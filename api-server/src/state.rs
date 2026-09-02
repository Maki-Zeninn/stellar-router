use crate::rate_limit::RateLimiter;
use dashmap::DashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::broadcast;

use crate::{rpc::SorobanRpcClient, types::TransactionStatusEvent};

pub const MAX_WS_CONNECTIONS: usize = 100;
pub const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 100;

#[derive(Clone)]
pub struct AppState {
    pub rpc: SorobanRpcClient,
    #[allow(dead_code)]
    pub execution_contract_id: String,
    pub router_core_contract_id: String,
    pub rate_limiter: RateLimiter,
    pub tx_status_tx: broadcast::Sender<TransactionStatusEvent>,
    pub tx_subscribers: Arc<DashMap<String, usize>>,
    pub ws_connection_count: Arc<AtomicUsize>,
}

impl AppState {
    pub fn new(
        rpc_url: String,
        execution_contract_id: String,
        router_core_contract_id: String,
        rate_limiter: RateLimiter,
        rpc_timeout_secs: u64,
    ) -> Self {
        let (tx_status_tx, _) = broadcast::channel(1000);
        Self {
            rpc: SorobanRpcClient::with_timeout(
                rpc_url,
                Some(router_core_contract_id.clone()),
                rpc_timeout_secs,
            ),

            execution_contract_id,
            router_core_contract_id,
            rate_limiter,
            tx_status_tx,
            tx_subscribers: Arc::new(DashMap::new()),
            ws_connection_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[allow(dead_code)]
    /// Broadcast a `TransactionStatusEvent` to every WebSocket subscriber
    /// that has registered interest in the event's `tx_id`.
    ///
    /// **Status: stub (no production caller).** A repo-wide grep for
    /// `broadcast_status` shows this method is invoked only from
    /// `api-server/src/tests.rs` — none of `handlers.rs`, `websocket.rs`,
    /// `rpc.rs`, or `main.rs` ever construct or send a
    /// `TransactionStatusEvent`. As a result, the WebSocket subscribe /
    /// unsubscribe machinery (`/ws`, `SubscribeMessage`,
    /// `MAX_SUBSCRIPTIONS_PER_CONNECTION`, `tx_subscribers`,
    /// `tx_status_tx`) is fully wired up and well tested, but in
    /// production **a real client connecting to `/ws` and subscribing
    /// to a `tx_id` will never receive a `status_update` message** —
    /// there is simply no producer anywhere in the running server.
    ///
    /// Wiring up a real producer is out of scope for this small change;
    /// in the meantime, this doc comment makes the stub state explicit
    /// so a future reader of `websocket.rs` in isolation is not misled
    /// into thinking the broadcast pipeline is working. See issue #1163.
    #[allow(dead_code)]
    pub fn broadcast_status(&self, event: TransactionStatusEvent) {
        let _ = self.tx_status_tx.send(event);
    }

    pub fn add_subscriber(&self, tx_id: String) {
        self.tx_subscribers
            .entry(tx_id)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

    pub fn remove_subscriber(&self, tx_id: &str) {
        if let Some(mut entry) = self.tx_subscribers.get_mut(tx_id) {
            if *entry > 1 {
                *entry -= 1;
            } else {
                drop(entry);
                self.tx_subscribers.remove(tx_id);
            }
        }
    }

    /// Returns true if a new connection was accepted, false if the limit is reached.
    pub fn try_acquire_ws_connection(&self) -> bool {
        self.ws_connection_count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                if current < MAX_WS_CONNECTIONS {
                    Some(current + 1)
                } else {
                    None
                }
            })
            .is_ok()
    }

    pub fn release_ws_connection(&self) {
        self.ws_connection_count.fetch_sub(1, Ordering::SeqCst);
    }
}
