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
  pub next_round: u64,
  pub min_proposal_number: Option<ProposalNumber>,
  pub accepted_proposal: Option<(ProposalNumber, String)>,
  pub chosen_value: Option<String>,
}

// Return the state in which the program starts.
pub fn initial_state() -> State {
  State {
    next_round: 0,
    min_proposal_number: None,
    accepted_proposal: None,
    chosen_value: None,
  }
}

#[cfg(test)]
mod tests {
  use crate::protocol::{
    generate_proposal_number, initial_state, ProposalNumber,
  };
  use std::net::{Ipv4Addr, SocketAddrV4};

  #[test]
  fn proposal_ord_round() {
    let pn1 = ProposalNumber {
      round: 0,
      proposer_ip: 1,
      proposer_port: 1,
    };

    let pn2 = ProposalNumber {
      round: 1,
      proposer_ip: 0,
      proposer_port: 0,
    };

    assert!(pn2 > pn1);
  }

  #[test]
  fn proposal_ord_proposer_ip() {
    let pn1 = ProposalNumber {
      round: 0,
      proposer_ip: 0,
      proposer_port: 1,
    };

    let pn2 = ProposalNumber {
      round: 0,
      proposer_ip: 1,
      proposer_port: 0,
    };

    assert!(pn2 > pn1);
  }

  #[test]
  fn proposal_ord_proposer_port() {
    let pn1 = ProposalNumber {
      round: 0,
      proposer_ip: 0,
      proposer_port: 0,
    };

    let pn2 = ProposalNumber {
      round: 0,
      proposer_ip: 0,
      proposer_port: 1,
    };

    assert!(pn2 > pn1);
  }

  #[test]
  fn first_proposal_number() {
    let mut state = initial_state();
    let address1 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000);
    let address2 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 2), 3001);
    let address3 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 3), 3002);
    let nodes = vec![address1, address2, address3];
    let pn = generate_proposal_number(&nodes, 1, &mut state);
    assert_eq!(pn.round, 0);
    assert_eq!(pn.proposer_ip, u32::from(*address2.ip()));
    assert_eq!(pn.proposer_port, address2.port());
  }

  #[test]
  fn second_proposal_number() {
    let mut state = initial_state();
    let nodes = vec![SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000)];
    let pn1 = generate_proposal_number(&nodes, 0, &mut state);
    let pn2 = generate_proposal_number(&nodes, 0, &mut state);
    assert!(pn2 > pn1);
  }
}
