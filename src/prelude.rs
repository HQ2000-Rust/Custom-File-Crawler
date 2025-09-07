#[cfg(any(feature = "async", feature = "lazy_store", doc))]
pub use crate::builder::marker::Async;
#[cfg(any(feature = "parallel", feature = "lazy_store", doc))]
pub use crate::builder::marker::NonAsync;
pub use crate::builder::{Crawler, context::NoContext};

#[cfg(any(feature = "parallel", doc))]
pub use rayon;
#[cfg(any(feature = "async", doc))]
pub use tokio;
