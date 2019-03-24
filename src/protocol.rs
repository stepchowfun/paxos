use futures::sync::mpsc::Sender;
use hyper::{
  client::HttpConnector, rt::Future, Body, Client, Method, Request,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

// A representation of a proposal number
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalNumber {
  pub round: u64,
  pub proposer: u32,
}

// We implement a custom ordering to ensure that round number takes precedence
// over proposer.
impl PartialOrd for ProposalNumber {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    if self.round == other.round {
      Some(self.proposer.cmp(&other.proposer))
    } else {
      Some(self.round.cmp(&other.round))
    }
  }
}

impl Ord for ProposalNumber {
  fn cmp(&self, other: &Self) -> Ordering {
    if self.round == other.round {
      self.proposer.cmp(&other.proposer)
    } else {
      self.round.cmp(&other.round)
    }
  }
}

// The state of the whole program is described by this struct.
#[derive(Serialize)]
pub struct State {
  pub min_proposal: Option<ProposalNumber>,
  pub accepted_proposal: Option<ProposalNumber>,
  pub accepted_value: Option<String>,
  pub max_round: u64,
  #[serde(skip)]
  pub quit_sender: Sender<()>,
}

// Return the state in which the program starts.
pub fn initial_state(quit_sender: Sender<()>) -> State {
  State {
    min_proposal: None,
    accepted_proposal: None,
    accepted_value: None,
    max_round: 0,
    quit_sender,
  }
}

// BEGIN PREPARE

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
  pub proposal: ProposalNumber,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareResponse {
  pub accepted_proposal: ProposalNumber,
  pub accepted_value: Option<String>,
}

pub fn prepare(
  request: &PrepareRequest,
  state: &mut State,
) -> PrepareResponse {
  match &state.min_proposal {
    Some(proposal) => {
      if request.proposal > *proposal {
        state.min_proposal = Some(request.proposal.clone());
      }
    }
    None => {
      state.min_proposal = Some(request.proposal.clone()); // [tag:accepted-proposal-exists]
    }
  }

  PrepareResponse {
    accepted_proposal: state.accepted_proposal.clone().unwrap(), // [ref:accepted-proposal-exists]
    accepted_value: state.accepted_value.clone(),
  }
}

// END PREPARE

// BEGIN ACCEPT

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptRequest {
  pub proposal: ProposalNumber,
  pub value: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptResponse {
  pub min_proposal: ProposalNumber,
}

pub fn accept(request: &AcceptRequest, state: &mut State) -> AcceptResponse {
  match &state.min_proposal {
    Some(proposal) => {
      if request.proposal >= *proposal {
        state.min_proposal = Some(request.proposal.clone());
        state.accepted_proposal = Some(request.proposal.clone());
        state.accepted_value = Some(request.value.clone());
      }
    }
    None => {
      state.min_proposal = Some(request.proposal.clone());
      state.accepted_proposal = Some(request.proposal.clone());
      state.accepted_value = Some(request.value.clone());
    }
  }

  AcceptResponse {
    min_proposal: state.min_proposal.clone().unwrap(), // Safe since accepts must follow at least one prepare
  }
}

// END ACCEPT

// BEGIN CHOOSE

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseRequest {
  pub value: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseResponse;

pub fn choose(request: &ChooseRequest, state: &mut State) -> ChooseResponse {
  println!("Consensus achieved: {}", request.value);
  let _ = state.quit_sender.try_send(()); // The first attempt (the only one that matters) should succeed
  ChooseResponse {}
}

// END CHOOSE

// BEGIN PROPOSE

pub fn propose(
  client: &Client<HttpConnector>,
  _nodes: &[crate::config::Node],
  value: &str,
) -> impl Future<Item = (), Error = ()> {
  let uri: hyper::Uri = "http://localhost:3000/choose".parse().unwrap();
  let payload = ChooseRequest {
    value: value.to_string(),
  };
  let req = Request::builder()
    .method(Method::POST)
    .uri(uri)
    .body(Body::from(bincode::serialize(&payload).unwrap())) // Safe since `ChooseRequest` has straightforward members
    .unwrap(); // Safe since we constructed a well-formed request
  client
    .request(req)
    .map(|_| ())
    .map_err(|e| eprintln!("Client error: {}", e))
}

// END PROPOSE

#[cfg(test)]
mod tests {}
