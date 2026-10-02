//! Forwards `tracing` events into the enclosing `fastrace` span.
//!
//! Spans are produced by `fastrace`, so `tracing` events would otherwise never
//! reach a trace: `tracing-opentelemetry` only ever exported them as events
//! attached to `tracing` spans, and there are none left. fastrace models the
//! same idea — the docs describe an `Event` as "a log record attached to a
//! span" — so this layer bridges the two and restores event/span correlation.
//!
//! It also covers third-party instrumentation. Dependencies such as `iroh` and
//! `quinn` emit `tracing` events; without this they are invisible to traces.
//!
//! fastrace discards events with no local parent, so the layer checks for one
//! first and does no field capture when there is nothing to attach to. Field
//! values are stringified because fastrace properties are
//! `Cow<'static, str>` — it has no typed attribute channel.

use fastrace::collector::SpanContext;
use fastrace::local::LocalSpan;
use fastrace::Event as FastraceEvent;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

/// Collects event fields as fastrace properties. `message` is pulled out
/// separately so it can name the event rather than appear as a property.
#[derive(Default)]
struct PropertyVisitor {
    message: Option<String>,
    properties: Vec<(&'static str, String)>,
}

impl PropertyVisitor {
    fn push(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.properties.push((field.name(), value));
        }
    }
}

impl Visit for PropertyVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, value.to_owned());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push(field, value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push(field, value.to_string());
    }
    fn record_i128(&mut self, field: &Field, value: i128) {
        self.push(field, value.to_string());
    }
    fn record_u128(&mut self, field: &Field, value: u128) {
        self.push(field, value.to_string());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push(field, value.to_string());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.push(field, value.to_string());
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.push(field, value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.push(field, format!("{value:?}"));
    }
}

pub struct FastraceEventLayer;

impl<S: Subscriber> Layer<S> for FastraceEventLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // No local parent means fastrace would drop the event, so skip the
        // field capture entirely rather than stringify for nothing.
        if SpanContext::current_local_parent().is_none() {
            return;
        }

        let mut visitor = PropertyVisitor::default();
        event.record(&mut visitor);

        let meta = event.metadata();
        let name = visitor.message.unwrap_or_else(|| meta.name().to_owned());
        let mut properties = visitor.properties;
        properties.push(("level", meta.level().to_string()));
        properties.push(("target", meta.target().to_owned()));

        LocalSpan::add_event(FastraceEvent::new(name).with_properties(|| properties));
    }
}
