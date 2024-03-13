mod app;
#[allow(clippy::module_inception)]
mod server;
mod shutdown;
mod utils;
pub use crate::server::server::*;
pub use crate::server::shutdown::shutdown_signal;
pub use crate::server::utils::{port_from_env_vars, DEFAULT_HOST, DEFAULT_PORT, DEFAULT_PORT_STR};
