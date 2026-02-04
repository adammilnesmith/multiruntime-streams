//! Spawner traits and implementations for controlling task placement across Tokio runtimes.

use std::future::Future;

#[cfg(feature = "tokio")]
use tokio::task::JoinHandle;

pub trait Spawner: Clone + Send + Sync + 'static {
    fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;
}

pub trait BlockingSpawner: Spawner {
    fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static;
}

pub trait IntoSpawner {
    type Spawner: Spawner;

    fn into_spawner(self) -> Self::Spawner;
}

#[cfg(feature = "tokio")]
mod tokio_impl {
    use super::*;
    use tokio::runtime::Handle;

    #[derive(Clone, Debug)]
    pub struct RuntimeSpawner {
        handle: Handle,
    }

    impl RuntimeSpawner {
        pub fn new(handle: Handle) -> Self {
            Self { handle }
        }

        pub fn handle(&self) -> &Handle {
            &self.handle
        }
    }

    impl Spawner for RuntimeSpawner {
        fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            self.handle.spawn(future)
        }
    }

    impl BlockingSpawner for RuntimeSpawner {
        fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
        where
            F: FnOnce() -> R + Send + 'static,
            R: Send + 'static,
        {
            self.handle.spawn_blocking(func)
        }
    }

    impl IntoSpawner for RuntimeSpawner {
        type Spawner = Self;

        fn into_spawner(self) -> Self::Spawner {
            self
        }
    }

    impl IntoSpawner for Handle {
        type Spawner = RuntimeSpawner;

        fn into_spawner(self) -> Self::Spawner {
            RuntimeSpawner::new(self)
        }
    }
}

#[cfg(feature = "tokio")]
pub use tokio_impl::RuntimeSpawner;
