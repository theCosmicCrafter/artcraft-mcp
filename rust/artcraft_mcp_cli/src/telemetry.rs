//! W3C Trace Context (SEP-414): extract `traceparent`/`tracestate`/`baggage`
//! from inbound request `_meta` into tracing spans, and inject them as headers
//! on outbound reqwest calls to the ArtCraft API.

use tracing::Span;

/// Extract W3C trace context fields from a request `_meta` object (if present)
/// and record them on the current span for correlation.
pub fn extract_trace_context(meta: Option<&serde_json::Value>) {
    let Some(meta) = meta else { return };
    let span = Span::current();
    for key in ["traceparent", "tracestate", "baggage"] {
        if let Some(value) = meta.get(key).and_then(|v| v.as_str()) {
            span.record(key, value);
        }
    }
}

/// Inject the current span's trace context into an outbound request builder,
/// if the incoming request carried a `traceparent`.
pub fn propagate_trace_context(
    builder: reqwest::RequestBuilder,
    meta: Option<&serde_json::Value>,
) -> reqwest::RequestBuilder {
    let Some(meta) = meta else { return builder };
    let mut builder = builder;
    for key in ["traceparent", "tracestate", "baggage"] {
        if let Some(value) = meta.get(key).and_then(|v| v.as_str()) {
            builder = builder.header(key, value);
        }
    }
    builder
}
