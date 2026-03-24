pub mod config;
pub mod dispatch;
pub mod identity;
pub mod agent;
pub mod handlers;
pub mod sources;
pub mod logging;
pub mod collected_message;
pub mod collector;

pub use config::Config;
pub use dispatch::{Dispatcher, DispatchAction, DispatchRule};
pub use identity::Identity;
pub use collector::MessageCollector;
