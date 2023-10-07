use lazy_static::lazy_static;
use prometheus::{opts, register_int_counter_vec, HistogramVec, IntCounterVec, IntGaugeVec};

// Discovery request response time bucket (in seconds)
const DISCOVERY_RESPONSE_TIME_BUCKETS: &[f64; 9] =
    &[0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0, 10.0, 60.0];

lazy_static! {
    // Reports the number of Instances visible to this node, grouped by Configuration and whether it is shared
    pub static ref INSTANCE_COUNT_METRIC: IntGaugeVec = prometheus::register_int_gauge_vec!(
        "akri_instance_count",
        "Akri Instance Count",
        &["configuration", "is_shared"])
        .expect("akri_instance_count metric can be created");
    // Reports the time to get discovery results, grouped by Configuration
    pub static ref DISCOVERY_RESPONSE_TIME_METRIC: HistogramVec = prometheus::register_histogram_vec!(
        "akri_discovery_response_time",
        "Akri Discovery Response Time",
        &["configuration"],
        DISCOVERY_RESPONSE_TIME_BUCKETS.to_vec()
        )
        .expect("akri_discovery_response_time metric can be created");
    // Reports the result of discover requests, grouped by Discovery Handler name and whether it is succeeded
    pub static ref DISCOVERY_RESPONSE_RESULT_METRIC: IntCounterVec = register_int_counter_vec!(
        opts!("akri_discovery_response_result", "Akri Discovery Response Result"),
        &["discovery_handler_name", "result"])
        .expect("akri_discovery_response_result metric can be created");
}
