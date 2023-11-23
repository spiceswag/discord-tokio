//! JSON accepting [Stream]s and [Sink]s as well as other IO necessities.

use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{ready, FutureExt, Sink, SinkExt, Stream, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use websockets::{Message, WebSocketError, WebSocketReadHalf, WebSocketWriteHalf};

use crate::{model::RawGatewayEvent, Error};

/// JSON-encoded values received from a WebSocket.
#[derive(Debug)]
pub struct JsonStream<T> {
    inner: WebSocketReadHalf,
    _t: PhantomData<T>,
}

impl<T> JsonStream<T> {
    /// Creates a new [`JsonStream`] by wrapping a [`WebSocketReadHalf`]
    pub fn new(read: WebSocketReadHalf) -> Self {
        Self {
            inner: read,
            _t: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> Stream for JsonStream<T> {
    type Item = Result<T, JsonStreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let message = match ready!(self.inner.poll_next_unpin(cx)) {
            Some(Ok(message)) => message,
            Some(Err(err)) => return Poll::Ready(Some(Err(JsonStreamError::Ws(err)))),
            None => return Poll::Ready(None),
        };

        Poll::Ready(Some(match message {
            Message::Text(str) => serde_json::from_str(&str).map_err(JsonStreamError::Json),
            Message::Binary(bin) => serde_json::from_slice(&bin).map_err(JsonStreamError::Json),
        }))
    }
}

/// JSON-encoded values sent to a WebSocket peer.
#[derive(Debug)]
pub struct JsonSink {
    inner: WebSocketWriteHalf,
}

impl JsonSink {
    /// Creates a new [`JsonSink`] by wrapping a [`WebSocketWriteHalf`]
    pub fn new(write: WebSocketWriteHalf) -> Self {
        Self { inner: write }
    }
}

impl<T: Serialize> Sink<T> for JsonSink {
    type Error = JsonStreamError;

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        let json = serde_json::to_string(&item).map_err(JsonStreamError::Json)?;
        self.inner
            .start_send_unpin(Message::Text(json))
            .map_err(JsonStreamError::Ws)
    }

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready_unpin(cx).map_err(JsonStreamError::Ws)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_flush_unpin(cx).map_err(JsonStreamError::Ws)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_close_unpin(cx).map_err(JsonStreamError::Ws)
    }
}

/// Errors occurring when pulling JSON messages from the WebSocket
#[derive(Debug)]
pub enum JsonStreamError {
    /// Failed to pull a message from the WebSocket.
    Ws(WebSocketError),
    /// Failed to decode a message as JSON.
    Json(serde_json::Error),
}

/// Share a Sink between many concurrent users, by offloading the actual sending
/// to a spawned task, and using channels to communicate between the two.
///
/// # Errors
///
/// If you encounter an unrecoverable error when trying to send something,
/// you should drop the shared sink in order to drop the actual sink.
///
/// # Memory Exhaustion
///
/// This implementation utilizes unbounded channels to perform communication.
/// As such it is vulnerable to memory exhaustion if items get sent
/// in an uncontrolled fashion.
#[derive(Debug)]
pub struct SharedSink<Si, T>
where
    Si: Sink<T>,
{
    channel: Option<mpsc::UnboundedSender<(T, oneshot::Sender<Result<(), Si::Error>>)>>,
    current: Vec<oneshot::Receiver<Result<(), Si::Error>>>,
}

impl<Si, T> SharedSink<Si, T>
where
    T: Send,
    Si: Sink<T> + Send + Unpin,
    Si::Error: Send,
{
    /// Create a shared sink by offloading sending to a spawned task.
    pub fn new(sink: Si) -> Self {
        let (send, receive) =
            mpsc::unbounded_channel::<(T, oneshot::Sender<Result<(), Si::Error>>)>();

        tokio::spawn(async move {
            let mut sink = sink;

            while let Some((item, reply)) = receive.recv().await {
                reply.send(sink.send(item).await);
            }
        });

        Self {
            channel: Some(send),
            current: vec![],
        }
    }
}

impl<Si, T> Sink<T> for SharedSink<Si, T>
where
    Si: Sink<T>,
{
    type Error = SharedSinkError<Si, T>;

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        let (send, receive) = oneshot::channel();

        if let Some(sender) = self.channel.as_mut() {
            // chain shutdown of threads if tokio is closing tasks.
            sender.send((item, send)).unwrap();
            self.current.push(receive);
        } else {
            return Err(SharedSinkError::SinkClosed);
        }

        Ok(())
    }

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        for send in self.current.iter_mut() {
            match send.poll_unpin(cx) {
                Poll::Ready(Ok(Err(err))) => {
                    return Poll::Ready(Err(SharedSinkError::SinkError(err)))
                }
                Poll::Ready(Err(_)) => return Poll::Ready(Err(SharedSinkError::SinkClosed)),
                Poll::Pending => return Poll::Pending,
                _ => {}
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.channel.take();
        Poll::Ready(Ok(()))
    }
}

impl<Si: Sink<T>, T> Clone for SharedSink<Si, T> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
            current: vec![],
        }
    }
}

/// Errors that can occur when sending to a shared sink.
#[derive(Debug)]
pub enum SharedSinkError<Si: Sink<T>, T> {
    SinkClosed,
    SinkError(Si::Error),
}

/// A stream over gateway events.
#[derive(Debug)]
pub struct GatewayEventStream {
    json: JsonStream<Value>,
}

impl GatewayEventStream {
    /// Construct a new gateway event stream
    /// by wrapping a stream over the raw JSON of said events
    pub fn new(json: JsonStream<Value>) -> Self {
        Self { json }
    }
}

impl Stream for GatewayEventStream {
    type Item = Result<RawGatewayEvent, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let message = match ready!(self.json.poll_next_unpin(cx)) {
            Some(Ok(message)) => message,
            Some(Err(JsonStreamError::Json(json_err))) => {
                return Poll::Ready(Some(Err(Error::Json(json_err))))
            }
            Some(Err(JsonStreamError::Ws(ws_err))) => {
                return Poll::Ready(Some(Err(Error::WebSocket(ws_err))))
            }
            None => return Poll::Ready(None),
        };

        Poll::Ready(Some(Ok(RawGatewayEvent::decode(message)?)))
    }
}
