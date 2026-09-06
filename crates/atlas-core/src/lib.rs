pub mod domain;
pub mod error;

pub use error::AtlasError;
pub type Result<T> = std::result::Result<T, AtlasError>;
