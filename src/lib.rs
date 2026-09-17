pub mod config;
pub mod dsp;
pub mod protocol;
pub mod radio;
pub mod server;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
