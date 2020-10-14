// Taken from iotedge:
//     https://raw.githubusercontent.com/Azure/iotedge/master/edgelet/iotedged/src/signal.rs
// iotedge added this credit:
//     Adapted from the conduit proxy signal handling:
//     https://github.com/runconduit/conduit/blob/master/proxy/src/signal.rs

use futures_old::Future;

pub type ShutdownSignal = Box<dyn Future<Item = (), Error = ()> + Send>;

/// Get shutdown signal to handle SIGINT and SIGTERM
pub fn shutdown() -> ShutdownSignal {
    imp::shutdown()
}

#[cfg(unix)]
mod imp {
    use super::ShutdownSignal;
    use futures_old::{future, Future, Stream};
    use log::trace;
    use std::fmt;
    use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

    pub(super) fn shutdown() -> ShutdownSignal {
        let signals = [SIGINT, SIGTERM].iter().map(|&sig| {
            Signal::new(sig)
                .flatten_stream()
                .into_future()
                .map(move |_| {
                    trace!("Received {}, starting shutdown", DisplaySignal(sig));
                })
        });
        let on_any_signal = future::select_all(signals)
            .map(|_| ())
            .map_err(|_| unreachable!("Signal never returns an error"));
        Box::new(on_any_signal)
    }

    /// This is used to store and handle specific shutdown signals
    #[derive(Clone, Copy)]
    struct DisplaySignal(i32);

    /// Implement Display for SIGINT and SIGTERM
    impl fmt::Display for DisplaySignal {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let s = match self.0 {
                SIGINT => "SIGINT",
                SIGTERM => "SIGTERM",
                other => return write!(f, "signal {}", other),
            };
            f.write_str(s)
        }
    }
}
