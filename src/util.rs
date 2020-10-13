use futures::{
    future::{err, ok},
    prelude::Future,
    stream::{self, Stream},
    Poll,
};
use hyper::{client::HttpConnector, Body, Client, Method, Request};
use serde::{de::DeserializeOwned, Serialize};
use std::net::SocketAddrV4;
use std::{
    cmp::min,
    time::{Duration, Instant},
};
use tokio::{fs::file::File, io::Error, prelude::FutureExt, timer::Delay};

// Duration constants
const EXPONENTIAL_BACKOFF_MIN: Duration = Duration::from_millis(100);
const EXPONENTIAL_BACKOFF_MAX: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

// Repeat a future until it succeeds with truncated binary exponential backoff on retries.
pub fn repeat<
    I: 'static + Send,
    R: 'static + Send + Future<Item = I, Error = ()>,
    C: 'static + Send + Fn() -> R,
>(
    constructor: C,
) -> impl Send + Future<Item = I, Error = ()> {
    fn repeat_rec<
        I: 'static + Send,
        R: 'static + Send + Future<Item = I, Error = ()>,
        C: 'static + Send + Fn() -> R,
    >(
        constructor: C,
        delay: Duration,
    ) -> impl Future<Item = I, Error = ()> + Send {
        constructor().then(move |result| {
            if let Ok(x) = result {
                Box::new(ok(x)) as Box<dyn Future<Item = I, Error = ()> + Send>
            } else {
                Box::new(
                    Delay::new(Instant::now() + delay)
                        .map_err(|_| ())
                        .and_then(move |_| {
                            repeat_rec(constructor, min(delay * 2, EXPONENTIAL_BACKOFF_MAX))
                        }),
                ) as Box<dyn Future<Item = I, Error = ()> + Send>
            }
        })
    }

    repeat_rec(constructor, EXPONENTIAL_BACKOFF_MIN)
}

// Send a message to all nodes. This function will automatically retry each request until it
// succeeds.
pub fn broadcast<
    P: 'static + Send + Sync + Clone + Serialize,
    R: 'static + Send + Sync + DeserializeOwned,
>(
    nodes: &[SocketAddrV4],
    client: &Client<HttpConnector>,
    endpoint: &str,
    payload: P,
) -> impl Send + Stream<Item = R, Error = ()> {
    nodes
        .iter()
        .map(|node| {
            let node = *node;
            let client = client.clone();
            let endpoint = endpoint.to_string();
            let payload = payload.clone();
            repeat(move || {
                client
                    .request(
                        Request::builder()
                            .method(Method::POST)
                            .uri(format!("http://{}{}", node, endpoint))
                            .body(Body::from(
                                // The `unwrap` is safe because serialization should never fail.
                                bincode::serialize(&payload).unwrap(),
                            ))
                            .unwrap(), // Safe since we constructed a well-formed request ,
                    )
                    .and_then(|response| {
                        response.into_body().concat2().map(|body| {
                            bincode::deserialize(&body.iter().cloned().collect::<Vec<u8>>())
                                .unwrap() // Safe under non-Byzantine conditions
                        })
                    })
                    .timeout(REQUEST_TIMEOUT)
                    .map_err(|_| ())
            })
            .into_stream()
        })
        .fold(
            Box::new(stream::empty()) as Box<dyn Stream<Item = R, Error = ()> + Send>,
            |acc, x| Box::new(acc.select(x)) as Box<dyn Stream<Item = R, Error = ()> + Send>,
        )
}

// Wait for a sufficient set of futures to finish, where the criteria for "sufficient" is provided
// by a closure.
pub fn when<I: 'static + Send, R: 'static + Send, K: 'static + Send + Fn(&[I]) -> Option<R>>(
    stream: impl 'static + Send + Stream<Item = I, Error = ()>,
    k: K,
) -> impl Future<Item = R, Error = ()> + Send {
    fn when_rec<I: 'static + Send, R: 'static + Send, K: 'static + Send + Fn(&[I]) -> Option<R>>(
        stream: impl 'static + Send + Stream<Item = I, Error = ()>,
        mut acc: Vec<I>,
        k: K,
    ) -> impl Send + Future<Item = R, Error = ()> {
        k(&acc).map_or_else(
            || {
                Box::new(stream.into_future().then(|result| match result {
                    Ok((x, s)) => x.map_or_else(
                        || Box::new(err(())) as Box<dyn Future<Item = R, Error = ()> + Send>,
                        |r| {
                            acc.push(r);
                            Box::new(when_rec(s, acc, k))
                                as Box<dyn Future<Item = R, Error = ()> + Send>
                        },
                    ),
                    Err((_, s)) => Box::new(when_rec(s, acc, k))
                        as Box<dyn Future<Item = R, Error = ()> + Send>,
                })) as Box<dyn Future<Item = R, Error = ()> + Send>
            },
            |result| Box::new(ok(result)) as Box<dyn Future<Item = R, Error = ()> + Send>,
        )
    }

    when_rec(stream, Vec::new(), k)
}

// The following provides fsync functionality in the form of a future.
struct FSyncFuture {
    pub file: File,
}

impl Future for FSyncFuture {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.file.poll_sync_all()
    }
}

pub fn fsync(file: File) -> impl Future<Item = (), Error = Error> {
    FSyncFuture { file }
}
