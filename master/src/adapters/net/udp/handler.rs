use std::sync::Arc;
use std::time::Duration;

use crate::services::{DeviceService, GameService};

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::connection::{
    INBOUND_QUEUE_CAPACITY, OUTBOUND_QUEUE_CAPACITY, handle_incoming, handle_outbound_queue,
    run_actor,
};

pub(super) struct WorkerHandler;

impl WorkerHandler {
    pub(super) async fn run(
        listener: Arc<UdpSocket>,
        worker_timeout: Duration,
        game: GameService,
        devices: DeviceService,
        shutdown: CancellationToken,
    ) {
        let (inbound_tx, inbound_rx) = mpsc::channel(INBOUND_QUEUE_CAPACITY);
        let (outbound_tx, outbound_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);

        let child_shutdown = shutdown.child_token();
        let mut tasks = JoinSet::new();
        tasks.spawn(handle_outbound_queue(Arc::clone(&listener), outbound_rx));
        tasks.spawn(handle_incoming(
            Arc::clone(&listener),
            inbound_tx,
            child_shutdown.clone(),
        ));
        tasks.spawn(run_actor(
            listener,
            worker_timeout,
            game,
            devices,
            inbound_rx,
            outbound_tx,
            child_shutdown.clone(),
        ));

        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                tracing::error!("[UDP] Background task failed: {error}");
            }
            child_shutdown.cancel();
        }
    }
}
