mod event;

pub use event::*;

type StdResult<T> = std::result::Result<T, Box<dyn std::error::Error + Sync + Send>>;
