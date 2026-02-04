//! Extension trait for Stream that adds runtime-aware operators.

use crate::DEFAULT_CAPACITY;
use crate::RuntimeSpawner;
use crate::spawner::IntoSpawner;
use futures_core::Stream;
use std::future::Future;

pub trait MultiRuntimeStreamExt: tokio_stream::StreamExt {
    fn poll_on<S>(self, spawner: S) -> PollOn<Self, S::Spawner>
    where
        Self: Sized + Send + 'static,
        Self::Item: Send + 'static,
        S: IntoSpawner,
    {
        self.poll_on_with(spawner, DEFAULT_CAPACITY)
    }

    /// Like `poll_on` but with custom channel capacity.
    fn poll_on_with<S>(self, spawner: S, capacity: usize) -> PollOn<Self, S::Spawner>
    where
        Self: Sized + Send + 'static,
        Self::Item: Send + 'static,
        S: IntoSpawner,
    {
        PollOn::new(self, spawner.into_spawner(), capacity)
    }

    fn flat_map<F, Fut, S>(
        self,
        mapper: F,
        spawner: S,
        concurrency: usize,
    ) -> FlatMap<Self, F, S::Spawner, Fut::Output>
    where
        Self: Sized + Send + 'static,
        Self::Item: Send + 'static,
        F: FnMut(Self::Item) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
        S: IntoSpawner,
    {
        FlatMap::new(self, mapper, spawner.into_spawner(), concurrency)
    }

    fn flat_map_result<F, Fut, S>(
        self,
        mapper: F,
        spawner: S,
        concurrency: usize,
    ) -> FlatMapResult<Self, F, S::Spawner, Fut::Output>
    where
        Self: Sized + Send + 'static,
        Self::Item: Send + 'static,
        F: FnMut(Self::Item) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
        S: IntoSpawner,
    {
        FlatMapResult::new(self, mapper, spawner.into_spawner(), concurrency)
    }

    fn on_current(self) -> PollOn<Self, RuntimeSpawner>
    where
        Self: Sized + Send + 'static,
        Self::Item: Send + 'static,
    {
        let spawner = RuntimeSpawner::new(
            tokio::runtime::Handle::try_current()
                .expect("on_current() called outside of a Tokio runtime context"),
        );
        self.poll_on(spawner)
    }
}

// Blanket implementation for all streams
impl<T: Stream + ?Sized> MultiRuntimeStreamExt for T {}

// Re-export operator types from crate::ops
pub use crate::ops::{FlatMap, FlatMapResult, PollOn};
