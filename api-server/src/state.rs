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
    ) -> Self {
        let (tx_status_tx, _) = broadcast::channel(1000);
        Self {
            rpc: SorobanRpcClient::new(rpc_url, Some(router_core_contract_id.clone())),
            execution_contract_id,
            router_core_contract_id,
            rate_limiter,
            tx_status_tx,
            tx_subscribers: Arc::new(DashMap::new()),
            ws_connection_count: Arc::new(AtomicUsize::new(0)),
        }
    }

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
