use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static GUARD: std::sync::OnceLock<WorkerGuard> = std::sync::OnceLock::new();

fn log_path() -> PathBuf {
    #[cfg(windows)]
    {
        let data = crate::config::paths::logs_dir();
        let _ = std::fs::create_dir_all(&data);
        data.join("slate.log")
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir().join("slate.log")
    }
}

pub fn init() {
    let path = log_path();
    let file_appender = tracing_appender::rolling::never(
        path.parent().unwrap_or(&std::env::temp_dir()),
        path.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("slate.log")),
    );
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = GUARD.set(guard);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,slate=debug"));

    let fmt_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init();
}
