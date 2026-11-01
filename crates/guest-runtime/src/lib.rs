//! Native Linux guest control. The assistant owns authorization and action intent.
pub mod actions;
pub mod audio;
pub mod cursor;
pub mod files;
pub mod home_identity;
pub mod imaging;
pub mod media;
pub mod notifications;
pub mod process;
pub mod runtime;
pub mod screen;
pub mod settings;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
pub use runtime::{DesktopRuntime, Response};
pub use settings::Settings;
