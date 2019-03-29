use futures::sync::mpsc::Sender;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, net::SocketAddrV4};

// A representation of a proposal number
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalNumber {
  pub round: u64,
  pub proposer_ip: u32,
  pub proposer_port: u16,
}

// We implement a custom ordering to ensure that round number takes precedence
// over proposer.
impl Ord for ProposalNumber {
  fn cmp(&self, other: &Self) -> Ordering {
    if self.round == other.round {
      if self.proposer_ip == other.proposer_ip {
        self.proposer_port.cmp(&other.proposer_port)
      } else {
        self.proposer_ip.cmp(&other.proposer_ip)
      }
    } else {
      self.round.cmp(&other.round)
    }
  }
}

// `Ord` requires `PartialOrd`.
impl PartialOrd for ProposalNumber {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

// This function generates a new proposal number.
pub fn generate_proposal_number(
  nodes: &[SocketAddrV4],
  node_index: usize,
  state: &mut State,
) -> ProposalNumber {
  let proposal_number = ProposalNumber {
    round: state.next_round,
    proposer_ip: u32::from(*nodes[node_index].ip()),
    proposer_port: nodes[node_index].port(),
  };
  state.next_round += 1;
  proposal_number
}

// The state of the whole program is described by this struct.
#[derive(Serialize)]
pub struct State {
  pub min_proposal_number: Option<ProposalNumber>,
  pub accepted_proposal: Option<(ProposalNumber, String)>,
  pub next_round: u64,
  #[serde(skip)]
  pub quit_sender: Sender<()>,
}

// Return the state in which the program starts.
pub fn initial_state(quit_sender: Sender<()>) -> State {
  State {
    min_proposal_number: None,
    accepted_proposal: None,
    next_round: 0,
    quit_sender,
  }
}

#[cfg(test)]
mod tests {}
