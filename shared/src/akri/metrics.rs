use log::info;
use prometheus::Encoder;
use warp::{Filter, Rejection, Reply};

/// Environment variable name for setting metrics port
const METRICS_PORT_LABEL: &str = "METRICS_PORT";

/// Reports an Akri component's latest custom Prometheus metrics along with
/// process metrics such as process_cpu_seconds_total, process_open_fds, etc, which are added by
/// default to the default Prometheus registry.
/// See https://prometheus.io/docs/instrumenting/writing_clientlibs/#process-metrics
/// for the entire list of default process metrics.
async fn metrics_handler() -> Result<impl Reply, Rejection> {
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();
    encoder
        .encode(&prometheus::gather(), &mut buffer)
        .expect("couldn't encode prometheus metrics");
    let res =
        String::from_utf8(buffer).expect("prometheus metrics could not be converted to String");
    Ok(res)
}

/// Serves prometheus metrics over a web service at /metrics
pub async fn run_metrics_server() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>
{
    let port = match std::env::var(METRICS_PORT_LABEL) {
        Ok(p) => p.parse::<u16>()?,
        Err(_) => 8080,
    };
    info!("starting metrics server on port {} at /metrics", port);
    let metrics_route = warp::path!("metrics").and_then(metrics_handler);
    warp::serve(metrics_route).run(([0, 0, 0, 0], port)).await;
    Ok(())
}
