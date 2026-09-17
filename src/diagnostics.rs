//! Optional diagnostic sink for embedding the server in a GUI.
use std::sync::Mutex;

type Sink = fn(&str);
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

pub fn set_sink(sink: Sink) {
    *SINK.lock().unwrap_or_else(|p| p.into_inner()) = Some(sink);
}

pub fn emit(args: std::fmt::Arguments<'_>) {
    let message = args.to_string();
    eprintln!("{message}");
    let sink = *SINK.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(sink) = sink {
        sink(&message);
    }
}

#[macro_export]
macro_rules! diagnostic {
    ($($args:tt)*) => { $crate::diagnostics::emit(format_args!($($args)*)) };
}
