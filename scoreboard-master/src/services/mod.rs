mod device;
mod game;

use std::time::Duration;

pub use device::{DeviceGameAssignment, DeviceService};
pub use game::{GameFrame, GameService};

pub(super) async fn retry_blocking<F>(label: &str, op: F) -> bool
where
    F: FnOnce() -> Result<(), String> + Send + Sync + Clone + 'static,
{
    let delays = [Duration::from_millis(50), Duration::from_millis(100)];
    let max_attempts = delays.len() + 1;

    for attempt in 0..max_attempts {
        let task_op = op.clone();
        match tokio::task::spawn_blocking(task_op).await {
            Ok(Ok(())) => return true,
            Ok(Err(error)) => {
                if let Some(delay) = delays.get(attempt) {
                    tracing::warn!(
                        "{label} failed (attempt {}): {error}; retrying in {}ms",
                        attempt + 1,
                        delay.as_millis()
                    );
                    tokio::time::sleep(*delay).await;
                } else {
                    tracing::error!("{label} failed after {max_attempts} attempts: {error}");
                }
            }
            Err(error) => {
                tracing::error!("{label} task panicked: {error}");
                return false;
            }
        }
    }

    false
}
