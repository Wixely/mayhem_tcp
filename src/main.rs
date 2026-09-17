use mayhem_tcp::{Result, config::Config, server};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[cfg(windows)]
mod windows;

fn main() -> Result<()> {
    let Some(config) = Config::parse()? else {
        return Ok(());
    };
    if config.service {
        #[cfg(windows)]
        return windows::dispatch();
        #[cfg(not(windows))]
        return Err("Use systemd on Linux; --service is for Windows SCM".into());
    }
    let shutdown = Arc::new(AtomicBool::new(false));
    let signal = shutdown.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    server::run(config, shutdown)
}
