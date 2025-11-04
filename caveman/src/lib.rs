use std::{
    error::Error,
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    net::{TcpListener, TcpStream},
    runtime::Handle,
    signal::unix::{SignalKind, signal},
    time::{Duration, sleep},
};

use hyper::{
    body::{Body, Frame, SizeHint},
    server::conn::http1::Builder,
};

use hyper_util::{rt::TokioIo, server::graceful::GracefulShutdown};

pub use bytes::{Bytes, BytesMut};
pub use http::{self, Response};
pub use hyper::{body::Incoming, service::service_fn};

// https://github.com/hyperium/hyper/issues/3746
/// An adapter over Bytes that implements hyper::body::Body
pub struct BodyBytes(Option<Bytes>);

pub type Request = hyper::Request<Incoming>;

// This would help cut code noise down, but doing so makes the
// builder disappear (it's only implemented for Response<()>)
// pub type Response = http::Response<BodyBytes>;

pub async fn serve<B, S>(listener: TcpListener, service: S)
where
    S: hyper::service::Service<Request, Response = Response<B>> + Clone + Send + 'static,
    S::Error: Into<Box<dyn Error + Send + Sync>>,
    S::Future: Send,
    B: Body + Send + 'static,
    B::Error: Into<Box<dyn Error + Send + Sync>>,
    B::Data: Send,
{
    let graceful = GracefulShutdown::new();
    let handle = Handle::current();

    // Extracted out of the accept loop because
    // tokio::select!{} and rustfmt don't play
    let handle_accept = |result: io::Result<(TcpStream, SocketAddr)>| {
        match result {
            Ok((stream, addr)) => {
                let conn = Builder::new().serve_connection(TokioIo::new(stream), service.clone());
                let conn = graceful.watch(conn);
                handle.spawn(async move {
                    // client disconnected, usually
                    if let Err(e) = conn.await {
                        eprintln!("error serving {addr}: {e}");
                    }
                    // done
                });
            }
            Err(err) => {
                eprintln!("Accept error: {err}");
            }
        }
    };

    loop {
        tokio::select! {
            biased;
            _ = shutdown_signal() => {
                // The accept future might be ready too, but it's
                // cancel safe (i.e. accept(2) only happens when
                // you poll and it yields Poll::Ready) so nothing
                // is lost
                eprintln!("Shutdown initiated. {} pending requests", graceful.count());
                drop(listener);
                break;
            }
            result = listener.accept() => {
                handle_accept(result);
            }
        }
    }

    tokio::select! {
        _ = graceful.shutdown() => {
            eprintln!("Graceful shutdown complete");
        },
        _ = sleep(Duration::from_secs(5)) => {
            eprintln!("Timed out waiting for pending clients");
        }
    };
}

async fn shutdown_signal() -> io::Result<()> {
    let mut sigterm = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            eprintln!("Received SIGINT");
        },
        _ = sigterm.recv() => {
            eprintln!("Received SIGTERM");
        },
    }
    Ok(())
}

impl<T: Into<Bytes>> From<T> for BodyBytes {
    fn from(value: T) -> Self {
        Self::from(value)
    }
}

impl BodyBytes {
    pub fn from<T: Into<Bytes>>(value: T) -> Self {
        let bytes = value.into();
        if bytes.is_empty() {
            Self(None)
        } else {
            Self(Some(bytes))
        }
    }
}

impl Body for BodyBytes {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if let Some(data) = self.0.take() {
            Poll::Ready(Some(Ok(Frame::data(data))))
        } else {
            Poll::Ready(None)
        }
    }

    fn size_hint(&self) -> SizeHint {
        let len = self.0.as_ref().map(|bytes| bytes.len()).unwrap_or_default() as u64;
        SizeHint::with_exact(len)
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_none()
    }
}

pub fn parse_qs(input: &str) -> impl Iterator<Item = Result<(&str, &str), &str>> {
    Parser::new(input)
}

struct Parser<'a> {
    input: &'a str,
    done: bool,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            done: input.is_empty(),
        }
    }

    fn next(&mut self) -> Option<Result<(&'a str, &'a str), &'a str>> {
        if self.done {
            None
        } else if let Some((key, rest)) = self.input.split_once('=') {
            if key.is_empty() || key.starts_with('&') {
                self.done = true;
                Some(Err(self.input))
            } else if let Some((value, rest)) = rest.split_once('&') {
                self.input = rest;
                self.done = rest.is_empty();
                Some(Ok((key, value)))
            } else {
                self.done = true;
                Some(Ok((key, rest)))
            }
        } else {
            self.done = true;
            Some(Err(self.input))
        }
    }
}

impl<'input> Iterator for Parser<'input> {
    type Item = Result<(&'input str, &'input str), &'input str>;

    fn next(&mut self) -> Option<Self::Item> {
        Self::next(self)
    }
}

// I want to restrict service_fn to minimize errors at a distance
// (i.e.: service_fn works but the future just can't be used
// as a service for serve_connection())
//
// But the typespec barf bellow yields a Service with a future
// that isn't Send and idk what's missing...
// The only place there isn't Send in this soup is in the
// function name lol
//
// I think it's related to this:
//
// https://github.com/rust-lang/impl-trait-initiative/issues/21
//
// The solution is returning a concrete type much like hyper's
// ServiceFn afaict
//
// pub fn service_fn<F, B, E, S>(
//     f: F,
// ) -> impl hyper::service::Service<Request, Response = Response<B>, Error = E> + Clone + Send + 'static
// where
//     F: Fn(Request) -> S + Clone + Send + 'static,
//     B: hyper::body::Body + Send + 'static,
//     B::Error: Into<Box<dyn Error + Send + Sync>>,
//     E: Into<Box<dyn Error + Send + Sync>>,
//     S: Future<Output = Result<Response<B>, E>> + Send,
// {
//     hyper::service::service_fn(f)
// }

#[cfg(test)]
mod tests {

    use super::Parser;

    #[test]
    fn parser_works() {
        let mut iter = Parser::new("foo=bar&baz=bow");
        assert_eq!(Some(Ok(("foo", "bar"))), iter.next());
        assert_eq!(Some(Ok(("baz", "bow"))), iter.next());
        assert_eq!(None, iter.next());

        let mut iter = Parser::new("foo=&bar=");
        assert_eq!(Some(Ok(("foo", ""))), iter.next());
        assert_eq!(Some(Ok(("bar", ""))), iter.next());
        assert_eq!(None, iter.next());

        let bads = [" ", "bad input", "=", "=&", "&="];
        for bad in bads {
            let mut iter = Parser::new(bad);
            assert!(iter.next().transpose().is_err(), "should fail: {}", bad);
        }

        let mut iter = Parser::new("foo=bar&&");
        assert_eq!(Some(Ok(("foo", "bar"))), iter.next());
        assert!(
            iter.next().transpose().is_err(),
            "won't try to handle lousy input"
        );
    }

    #[test]
    fn parser_is_done_after_err() {
        let mut iter = Parser::new("=&foo=bar");
        assert!(iter.next().transpose().is_err());
        assert_eq!(None, iter.next());
    }
}
