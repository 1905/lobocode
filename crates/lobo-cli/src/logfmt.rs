use std::{
    collections::BTreeMap,
    fmt,
    io::Write,
    sync::{Arc, Mutex},
};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, MakeWriter, format::Writer},
    registry::LookupSpan,
};

#[derive(Clone)]
pub struct SharedWriter(pub Arc<Mutex<Box<dyn Write + Send>>>);
impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}
impl<'a> MakeWriter<'a> for SharedWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

pub struct ZerologConsole {
    pub color: bool,
    pub tz: chrono::FixedOffset,
}
#[derive(Default)]
struct Fields {
    message: String,
    fields: BTreeMap<String, String>,
}
fn quoted(value: &str) -> String {
    if value
        .bytes()
        .any(|b| b <= b' ' || b > b'~' || b == b'\\' || b == b'"')
    {
        serde_json::to_string(value).expect("string JSON cannot fail")
    } else {
        value.into()
    }
}
impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.into();
        } else {
            self.fields.insert(field.name().into(), quoted(value));
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields
                .insert(field.name().into(), format!("{value:?}"));
        }
    }
    fn record_i64(&mut self, f: &Field, v: i64) {
        self.fields.insert(f.name().into(), v.to_string());
    }
    fn record_u64(&mut self, f: &Field, v: u64) {
        self.fields.insert(f.name().into(), v.to_string());
    }
    fn record_f64(&mut self, f: &Field, v: f64) {
        self.fields.insert(f.name().into(), v.to_string());
    }
    fn record_bool(&mut self, f: &Field, v: bool) {
        self.fields.insert(f.name().into(), v.to_string());
    }
}
impl<S, N> FormatEvent<S, N> for ZerologConsole
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _: &FmtContext<'_, S, N>,
        mut w: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let (level, color) = match *event.metadata().level() {
            tracing::Level::ERROR => ("ERR", "31"),
            tracing::Level::WARN => ("WRN", "33"),
            tracing::Level::DEBUG => ("DBG", "35"),
            tracing::Level::TRACE => ("TRC", "35"),
            _ => ("INF", "32"),
        };
        write!(
            w,
            "{} ",
            chrono::Utc::now()
                .with_timezone(&self.tz)
                .format("%H:%M:%S")
        )?;
        if self.color {
            write!(w, "\x1b[{color}m{level}\x1b[0m")?;
        } else {
            write!(w, "{level}")?;
        }
        write!(w, " {}", fields.message)?;
        for (k, v) in fields.fields {
            write!(w, " {k}={v}")?;
        }
        writeln!(w)
    }
}
pub fn layer<W>(
    writer: W,
    color: bool,
) -> impl tracing_subscriber::Layer<tracing_subscriber::Registry>
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .event_format(ZerologConsole {
            color,
            tz: *chrono::Local::now().offset(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl Write for Buffer {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn sorted_fields_and_quoting() {
        let buffer = Buffer::default();
        let w = SharedWriter(Arc::new(Mutex::new(Box::new(buffer.clone()))));
        let sub = tracing_subscriber::fmt()
            .event_format(ZerologConsole {
                color: false,
                tz: chrono::FixedOffset::east_opt(0).unwrap(),
            })
            .with_writer(w)
            .finish();
        tracing::subscriber::with_default(sub, || {
            tracing::info!(
                phase = "download",
                detail = "a b",
                cost = 0.69,
                next = "→",
                "up"
            );
        });
        let text = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert_eq!(
            &text[9..],
            "INF up cost=0.69 detail=\"a b\" next=\"→\" phase=download\n"
        );
    }
}
