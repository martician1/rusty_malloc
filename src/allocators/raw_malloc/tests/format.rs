//! Ad hoc formatter for readable logs + layer for logging on span entry.

use std::fmt;

use nu_ansi_term::Color;
use tracing::{span, trace, debug, info, warn, error};
use tracing::{Event, Id, Level, Subscriber};
use tracing_subscriber::fmt::format::{DefaultFields, FormatEvent, FormatFields};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::{format, FormattedFields};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

#[derive(Default)]
pub struct RecordEntryLayer {
    fmt_fields: DefaultFields,
}

impl<S> Layer<S> for RecordEntryLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();
        let mut fields = FormattedFields::<DefaultFields>::new(String::new());
        self.fmt_fields
            .format_fields(fields.as_writer(), attrs)
            .unwrap();
        extensions.insert(fields)
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let metadata = span.metadata();
        let extensions = span.extensions();
        let formatted_fields = extensions.get::<FormattedFields<DefaultFields>>().unwrap();

        // event!() does not work with non-constant levels, this is a quick fix.
        match *metadata.level() {
            Level::TRACE => trace!(args = %formatted_fields, "Enter."),
            Level::DEBUG => debug!(args = %formatted_fields, "Enter."),
            Level::INFO => info!(args = %formatted_fields, "Enter."),
            Level::WARN => warn!(args = %formatted_fields, "Enter."),
            Level::ERROR => error!(args = %formatted_fields, "Enter.")
        }
    }
}

#[derive(Default)]
pub struct SimpleFormatter;

impl<S> Layer<S> for SimpleFormatter where S: Subscriber + for<'a> LookupSpan<'a> {}

impl<S, N> FormatEvent<S, N> for SimpleFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        let fmt_level = FmtLevel::new(metadata.level());
        write!(&mut writer, "{}: ", fmt_level)?;

        // Format all the spans in the event's span context.
        let span = ctx.lookup_current().unwrap();
        write!(writer, "{}: ", Color::Purple.paint(span.name()))?;

        // Write fields on the event
        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

struct FmtLevel<'a> {
    level: &'a Level,
}

impl<'a> FmtLevel<'a> {
    pub(crate) fn new(level: &'a Level) -> Self {
        Self { level }
    }
}

const TRACE_STR: &str = "TRACE";
const DEBUG_STR: &str = "DEBUG";
const INFO_STR: &str = " INFO";
const WARN_STR: &str = " WARN";
const ERROR_STR: &str = "ERROR";

impl<'a> fmt::Display for FmtLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self.level {
            Level::TRACE => write!(f, "{}", Color::Purple.paint(TRACE_STR)),
            Level::DEBUG => write!(f, "{}", Color::Blue.paint(DEBUG_STR)),
            Level::INFO => write!(f, "{}", Color::Green.paint(INFO_STR)),
            Level::WARN => write!(f, "{}", Color::Yellow.paint(WARN_STR)),
            Level::ERROR => write!(f, "{}", Color::Red.paint(ERROR_STR)),
        }
    }
}
