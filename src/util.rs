use futures::{
  future::{err, ok},
  prelude::*,
};
use hyper::{client::HttpConnector, Body, Client, Method, Request};
use serde::{de::DeserializeOwned, Serialize};
use std::net::SocketAddrV4;
use std::{
  cmp::min,
  time::{Duration, Instant},
};
use tokio::{fs::file::File, io::Error, prelude::*, timer::Delay};

// Duration constants
const EXPONENTIAL_BACKOFF_MIN: Duration = Duration::from_millis(100);
const EXPONENTIAL_BACKOFF_MAX: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

// Repeat a future until it succeeds with truncated binary exponential backoff
// on retries.
pub fn repeat<
  I: 'static + Send,
  C: 'static + Send + Fn() -> Box<dyn Future<Item = I, Error = ()> + Send>,
>(
  constructor: C,
) -> Box<dyn Future<Item = I, Error = ()> + Send> {
  fn repeat_rec<
    I: 'static + Send,
    C: 'static + Send + Fn() -> Box<dyn Future<Item = I, Error = ()> + Send>,
  >(
    constructor: C,
    delay: Duration,
  ) -> Box<dyn Future<Item = I, Error = ()> + Send> {
    Box::new(constructor().then(move |result| {
      if let Ok(x) = result {
        Box::new(ok(x)) as Box<dyn Future<Item = I, Error = ()> + Send>
      } else {
        Box::new(Delay::new(Instant::now() + delay).map_err(|_| ()).and_then(
          move |_| {
            repeat_rec(constructor, min(delay * 2, EXPONENTIAL_BACKOFF_MAX))
          },
        )) as Box<dyn Future<Item = I, Error = ()> + Send>
      }
    }))
  }

  repeat_rec(constructor, EXPONENTIAL_BACKOFF_MIN)
}

// Send a message to all nodes. This function will automatically retry each
// request until it succeeds.
pub fn broadcast<
  P: 'static + Send + Sync + Clone + Serialize,
  R: 'static + Send + Sync + DeserializeOwned,
>(
  nodes: &[SocketAddrV4],
  client: &Client<HttpConnector>,
  endpoint: &str,
  payload: P,
) -> Box<dyn Stream<Item = R, Error = ()> + Send> {
  nodes
    .iter()
    .map(|node| {
      let node = *node;
      let client = client.clone();
      let endpoint = endpoint.to_string();
      let payload = payload.clone();
      repeat(move || {
        Box::new(
          client
            .request(
              Request::builder()
                .method(Method::POST)
                .uri(format!("http://{}{}", node, endpoint))
                .body(Body::from(
                  // The `unwrap` is safe because serialization should never
                  // fail.
                  bincode::serialize(&payload).unwrap(),
                ))
                .unwrap(), // Safe since we constructed a well-formed request
            )
            .and_then(|response| {
              response.into_body().concat2().map(|body| {
                bincode::deserialize(
                  &body.iter().cloned().collect::<Vec<u8>>(),
                )
                .unwrap() // Safe under non-Byzantine conditions
              })
            })
            .timeout(REQUEST_TIMEOUT)
            .map_err(|_| ()),
        )
      })
      .into_stream()
    })
    .fold(Box::new(stream::empty()), |acc, x| Box::new(acc.select(x)))
}

// Wait for a sufficient set of futures to finish, where the criteria for
// "sufficient" is provided by a closure.
pub fn when<
  I: 'static + Send,
  R: 'static + Send,
  K: 'static + Send + Fn(&[I]) -> Option<R>,
>(
  stream: Box<dyn Stream<Item = I, Error = ()> + Send>,
  k: K,
) -> Box<dyn Future<Item = R, Error = ()> + Send> {
  fn when_rec<
    I: 'static + Send,
    R: 'static + Send,
    K: 'static + Send + Fn(&[I]) -> Option<R>,
  >(
    stream: Box<dyn Stream<Item = I, Error = ()> + Send>,
    mut acc: Vec<I>,
    k: K,
  ) -> Box<dyn Future<Item = R, Error = ()> + Send> {
    if let Some(result) = k(&acc) {
      Box::new(ok(result))
    } else {
      Box::new(stream.into_future().then(|result| match result {
        Ok((x, s)) => {
          if let Some(r) = x {
            acc.push(r);
            when_rec(s, acc, k)
          } else {
            Box::new(err(()))
          }
        }
        Err((_, s)) => when_rec(s, acc, k),
      }))
    }
  }

  when_rec(stream, Vec::new(), k)
}

// The following provides fsync functionality in the form of a future.
pub struct FSyncFuture {
  pub file: File,
}

impl Future for FSyncFuture {
  type Item = ();
  type Error = Error;

  fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
    self.file.poll_sync_all()
  }
}

pub fn fsync(file: File) -> FSyncFuture {
  FSyncFuture { file: file }
}
