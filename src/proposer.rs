use crate::acceptor::{PrepareRequest, PrepareResponse};
use crate::protocol::{generate_proposal_number, State};
use futures::{
  future::{err, ok},
  prelude::*,
};
use hyper::{client::HttpConnector, Body, Client, Method, Request};
use serde::{de::DeserializeOwned, Serialize};
use std::{
  net::SocketAddrV4,
  sync::{Arc, RwLock},
  time::Duration,
};
use tokio::{prelude::*, timer::timeout};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

// Wait for a sufficient set of futures to finish.
fn when<
  I: 'static + Send,
  F: 'static + Send + Future<Item = I, Error = ()>,
  R: 'static + Send,
  K: 'static + Send + Fn(&[I]) -> Option<R>,
>(
  futures: Vec<F>,
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

  when_rec(
    futures
      .into_iter()
      .map(|future| future.into_stream())
      .fold(Box::new(stream::empty()), |acc, x| Box::new(acc.select(x))),
    Vec::new(),
    k,
  )
}

// Repeat a future until it succeeds.
// TODO: Add exponential backoff.
fn repeat<
  I: 'static + Send,
  E: 'static + Send,
  C: 'static + Send + Fn() -> Box<dyn Future<Item = I, Error = E> + Send>,
>(
  constructor: C,
) -> Box<dyn Future<Item = I, Error = E> + Send> {
  Box::new(constructor().then(move |result| {
    if let Ok(x) = result {
      Box::new(ok(x))
    } else {
      constructor()
    }
  }))
}

// Send a message to all nodes.
fn broadcast<
  P: 'static + Send + Sync + Clone + Serialize,
  R: 'static + Send + Sync + DeserializeOwned,
>(
  nodes: &[SocketAddrV4],
  client: &Client<HttpConnector>,
  endpoint: &str,
  payload: P,
) -> Vec<Box<dyn Future<Item = R, Error = timeout::Error<hyper::Error>> + Send>>
{
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
                .uri(format!("http://{}/{}", node, endpoint))
                .body(Body::from(bincode::serialize(&payload).unwrap())) // Serialization is safe.
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
            .timeout(REQUEST_TIMEOUT),
        )
          as Box<
            dyn Future<Item = R, Error = timeout::Error<hyper::Error>> + Send,
          >
      })
    })
    .collect()
}

pub fn propose(
  client: &Client<HttpConnector>,
  nodes: &[SocketAddrV4],
  node_index: usize,
  value: &str,
  state: &Arc<RwLock<State>>,
) -> Box<dyn Future<Item = (), Error = ()> + Send> {
  // Generate a new proposal number.
  let proposal_number = {
    let mut state_borrow = state.write().unwrap(); // Safe since it can only fail if a panic already happened
    generate_proposal_number(nodes, node_index, &mut state_borrow)
  };

  // Send a prepare message to all the nodes.
  let prepares = broadcast(
    nodes,
    client,
    "prepare",
    PrepareRequest {
      proposal_number: proposal_number.clone(),
    },
  )
  .into_iter()
  .map(
    |future: Box<dyn Future<Item = PrepareResponse, Error = _> + Send>| {
      future.map_err(|_| ())
    },
  )
  .collect::<Vec<_>>();

  // Wait for a majority of the nodes to respond.
  let majority = prepares.len() / 2 + 1;
  let value = value.to_string();
  Box::new(
    when(prepares, move |responses| {
      // TODO: Check the `min_proposal_number` from the responses.
      if responses.len() < majority {
        // We don't have a majority yet. Wait for more responses.
        None
      } else {
        // We have a majority. See if there were any existing proposals.
        let accepted_proposal = responses
          .iter()
          .filter_map(|response| response.accepted_proposal.clone())
          .max_by_key(|accepted_proposal| accepted_proposal.0.clone());
        if let Some(proposal) = accepted_proposal {
          // There was an existing proposal. Use that.
          Some(proposal.1.clone())
        } else {
          // Propose the given value.
          Some(value.to_string())
        }
      }
    })
    .and_then(|proposal| {
      println!("Proposing this value: {}", proposal);
      ok(())
    }),
  )
}

#[cfg(test)]
mod tests {}
