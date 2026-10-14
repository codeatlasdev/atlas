pub mod channel;
pub mod crash;
pub mod domain;
pub mod error;
pub mod github;
pub mod login_env;
pub mod paths;
pub mod util;

pub use error::AtlasError;
pub type Result<T> = std::result::Result<T, AtlasError>;
