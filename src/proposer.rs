use crate::acceptor::{
  AcceptRequest, AcceptResponse, ChooseRequest, ChooseResponse,
  PrepareRequest, PrepareResponse, ACCEPT_ENDPOINT, CHOOSE_ENDPOINT,
  PREPARE_ENDPOINT,
};
use crate::protocol::{generate_proposal_number, State};
use crate::util::{broadcast, when};
use futures::{future::ok, prelude::*};
use hyper::{client::HttpConnector, Client};
use rand::{thread_rng, Rng};
use std::{
  net::SocketAddrV4,
  sync::{Arc, RwLock},
  time::{Duration, Instant},
};
use tokio::timer::Delay;

// Duration constants
const RESTART_DELAY_MIN: Duration = Duration::from_millis(0);
const RESTART_DELAY_MAX: Duration = Duration::from_millis(100);

pub fn propose(
  client: &Client<HttpConnector>,
  nodes: &[SocketAddrV4],
  node_index: usize,
  value: &str,
  state: Arc<RwLock<State>>,
) -> Box<dyn Future<Item = (), Error = ()> + Send> {
  // Clone some data that will outlive this function.
  let value = value.to_string();
  let value_for_choose = value.clone();
  let nodes = nodes.to_vec();
  let client = client.clone();
  let nodes = nodes.clone();

  // Generate a new proposal number.
  let proposal_number = {
    let mut state_borrow = state.write().unwrap(); // Safe since it can only fail if a panic already happened
    generate_proposal_number(&nodes, node_index, &mut state_borrow)
  };

  // Send a prepare message to all the nodes.
  println!("Preparing this value: {}", value);
  let prepares = broadcast(
    &nodes,
    &client,
    PREPARE_ENDPOINT,
    PrepareRequest {
      proposal_number: proposal_number.clone(),
    },
  );

  // Wait for a majority of the nodes to respond.
  let majority = nodes.len() / 2 + 1;
  Box::new(
    when(prepares, move |responses: &[PrepareResponse]| {
      // Check if we have a quorum.
      if responses.len() < majority {
        // We don't have a quorum yet. Wait for more responses.
        None
      } else {
        // We have a quorum. See if there were any existing proposals.
        let accepted_proposal = responses
          .iter()
          .filter_map(|response| response.accepted_proposal.clone())
          .max_by_key(|accepted_proposal| accepted_proposal.0.clone());
        if let Some(proposal) = accepted_proposal {
          // There was an existing proposal. Use that.
          Some(proposal.1.clone())
        } else {
          // Propose the given value.
          Some(value.clone())
        }
      }
    })
    .and_then(move |proposal| {
      // Clone some data that will outlive this function.
      let proposal_for_choose = proposal.clone();

      // Send an accept message to all the nodes.
      println!("Proposing this value: {}", proposal);
      let accepts = broadcast(
        &nodes,
        &client,
        ACCEPT_ENDPOINT,
        AcceptRequest {
          proposal: (proposal_number.clone(), proposal),
        },
      );

      when(accepts, move |responses: &[AcceptResponse]| {
        // Check if we have a quorum.
        if responses.len() < majority {
          // We don't have a quorum yet. Wait for more responses.
          None
        } else {
          // We have a quorum. Check that there were no rejections.
          if responses
            .iter()
            .all(|response| response.min_proposal_number == proposal_number)
          {
            Some(true)
          } else {
            Some(false)
          }
        }
      })
      .and_then(move |succeeded| {
        if succeeded {
          // Consensus achieved. Notify all the nodes.
          Box::new(
            broadcast(
              &nodes,
              &client,
              CHOOSE_ENDPOINT,
              ChooseRequest {
                value: proposal_for_choose,
              },
            )
            .fold((), |_, _: ChooseResponse| ok(())),
          ) as Box<Future<Item = (), Error = ()> + Send>
        } else {
          // Paxos failed. Start over.
          Box::new(
            Delay::new(
              Instant::now()
                + thread_rng().gen_range(RESTART_DELAY_MIN, RESTART_DELAY_MAX),
            )
            .map_err(|_| ())
            .and_then(move |_| {
              propose(&client, &nodes, node_index, &value_for_choose, state)
            }),
          ) as Box<Future<Item = (), Error = ()> + Send>
        }
      })
    }),
  )
}

#[cfg(test)]
mod tests {}
