pub mod spawner;

pub mod spawners;

mod ops;

mod ext;

pub use ext::MultiRuntimeStreamExt;

pub use spawner::{BlockingSpawner, IntoSpawner, RuntimeSpawner, Spawner};

/// Default capacity for bounded channels in operators
pub const DEFAULT_CAPACITY: usize = 256;
