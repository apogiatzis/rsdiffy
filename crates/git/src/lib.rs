pub mod commits;
pub mod diff;
pub mod error;
pub mod exec;
pub mod repo;
pub mod status;
pub mod tree;
pub mod types;

pub use error::{GitError, Result};
pub use types::*;
