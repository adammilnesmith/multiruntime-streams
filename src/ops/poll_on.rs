use crate::spawner::Spawner;
use futures_core::Stream;
use futures_util::StreamExt as _;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

pub struct PollOn<S: Stream, Sp> {
    receiver: mpsc::Receiver<S::Item>,
    cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
    _phantom: std::marker::PhantomData<Sp>,
}

impl<S, Sp> PollOn<S, Sp>
where
    S: Stream + Send + 'static,
    S::Item: Send + 'static,
    Sp: Spawner,
{
    pub(crate) fn new(stream: S, spawner: Sp, capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

        spawner.spawn(async move {
            let mut stream = Box::pin(stream).take_until(cancel_rx);
            while let Some(item) = stream.next().await {
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });

        Self {
            receiver: rx,
            cancel_tx: Some(cancel_tx),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, Sp> Stream for PollOn<S, Sp>
where
    S: Stream,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(item) => Poll::Ready(item),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S, Sp> Drop for PollOn<S, Sp>
where
    S: Stream,
{
    fn drop(&mut self) {
        if let Some(cancel_tx) = self.cancel_tx.take() {
            let _ = cancel_tx.send(());
        }
    }
}

// Unpin is safe because we don't expose any pinned fields
impl<S: Stream, Sp> Unpin for PollOn<S, Sp> {}
