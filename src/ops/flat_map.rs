use crate::spawner::Spawner;
use futures_core::Stream;
use futures_util::StreamExt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct FlatMap<S: Stream, F, Sp, T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, tokio::task::JoinError>> + Send>>,
    _phantom: std::marker::PhantomData<(S, F, Sp)>,
}

impl<S, F, Fut, Sp> FlatMap<S, F, Sp, Fut::Output>
where
    S: Stream + Send + 'static,
    S::Item: Send + 'static,
    F: FnMut(S::Item) -> Fut + Send + 'static,
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
    Sp: Spawner,
{
    pub(crate) fn new(stream: S, mut mapper: F, spawner: Sp, concurrency: usize) -> Self {
        let inner = Box::pin(
            stream
                .map(move |item| {
                    let fut = mapper(item);
                    spawner.spawn(fut)
                })
                .buffered(concurrency),
        );

        Self {
            inner,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, F, Sp, T> Stream for FlatMap<S, F, Sp, T>
where
    S: Stream,
    T: 'static,
{
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(value))) => Poll::Ready(Some(value)),
            Poll::Ready(Some(Err(e))) => {
                panic!("flat_map task failed: {:?}", e);
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

// Unpin is safe because we Box::pin the inner stream
impl<S: Stream, F, Sp, T> Unpin for FlatMap<S, F, Sp, T> {}

pub struct FlatMapResult<S: Stream, F, Sp, T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, tokio::task::JoinError>> + Send>>,
    _phantom: std::marker::PhantomData<(S, F, Sp)>,
}

impl<S, F, Fut, Sp> FlatMapResult<S, F, Sp, Fut::Output>
where
    S: Stream + Send + 'static,
    S::Item: Send + 'static,
    F: FnMut(S::Item) -> Fut + Send + 'static,
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
    Sp: Spawner,
{
    pub(crate) fn new(stream: S, mut mapper: F, spawner: Sp, concurrency: usize) -> Self {
        let mapped = stream.map(move |item| {
            let fut = mapper(item);
            spawner.spawn(fut)
        });

        let inner = Box::pin(mapped.buffered(concurrency));

        Self {
            inner,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, F, Sp, T> Stream for FlatMapResult<S, F, Sp, T>
where
    S: Stream,
    T: 'static,
{
    type Item = Result<T, tokio::task::JoinError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

// Unpin is safe because we Box::pin the inner stream
impl<S: Stream, F, Sp, T> Unpin for FlatMapResult<S, F, Sp, T> {}
