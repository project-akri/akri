use std::sync::Arc;

use futures::stream::{AbortHandle, Abortable};
use tokio::{signal::unix::SignalKind, sync::watch};

#[derive(Clone)]
pub struct Stopper {
    state: Arc<watch::Sender<bool>>,
}

impl Stopper {
    pub fn new() -> Self {
        let (state, _) = watch::channel(false);
        let s = Self {
            state: Arc::new(state),
        };
        let local_s = s.clone();
        tokio::spawn(async move {
            let mut signal = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = local_s.stopped() => {},
                _ = signal.recv() => local_s.stop()
            }
        });
        s
    }

    pub fn stop(&self) {
        self.state.send_replace(true);
    }

    pub fn is_stopped(&self) -> bool {
        *self.state.borrow()
    }

    pub async fn stopped(&self) {
        let mut r = self.state.subscribe();
        if !*r.borrow_and_update() {
            let _ = r.changed().await;
        }
    }

    pub fn make_abortable<T>(&self, inner: T) -> Abortable<T> {
        let (handle, reg) = AbortHandle::new_pair();
        let local_self = self.clone();
        tokio::spawn(async move {
            local_self.stopped().await;
            handle.abort();
        });
        Abortable::new(inner, reg)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_stopper() {
        let stopper = Stopper::new();
        assert!(!stopper.is_stopped());
        assert!(
            tokio::time::timeout(Duration::from_secs(2), stopper.stopped())
                .await
                .is_err()
        );
        let local_stopper = stopper.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            local_stopper.stop()
        });
        assert!(
            tokio::time::timeout(Duration::from_secs(2), stopper.stopped())
                .await
                .is_ok()
        );
        assert!(stopper.is_stopped());
    }

    #[tokio::test]
    async fn test_make_abortable() {
        let stopper = Stopper::new();
        let abortable = stopper.make_abortable(async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            true
        });
        assert!(!abortable.is_aborted());
        assert_eq!(abortable.await, Ok(true));

        let abortable = stopper.make_abortable(async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            true
        });
        stopper.stop();
        assert!(abortable.await.is_err());
    }
}
