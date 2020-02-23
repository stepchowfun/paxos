use crate::util::fsync;
use futures::{
    future::{err, ok},
    prelude::Future,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    io::{Error, ErrorKind::InvalidData},
    path::Path,
    sync::{Arc, RwLock},
};
use tokio::{fs, fs::File};

// A representation of a proposal number
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalNumber {
    pub round: u64,
    pub proposer_ip: u32,
    pub proposer_port: u16,
}

// We implement a custom ordering to ensure that round number takes precedence over proposer.
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

// The state of the whole program is described by this struct.
#[derive(Deserialize, Serialize)]
pub struct State {
    pub next_round: u64,
    pub min_proposal_number: Option<ProposalNumber>,
    pub accepted_proposal: Option<(ProposalNumber, String)>,
    pub chosen_value: Option<String>,
}

// Return the state in which the program starts.
pub fn initial() -> State {
    State {
        next_round: 0,
        min_proposal_number: None,
        accepted_proposal: None,
        chosen_value: None,
    }
}

// Write the state to a file.
pub fn write(state: &State, path: &Path) -> impl Future<Item = (), Error = Error> {
    // Clone some data that will outlive this function.
    let path = path.to_owned();

    // The `unwrap` is safe because serialization should never fail.
    let payload = bincode::serialize(&state).unwrap();

    // The `unwrap` is safe due to [ref:data_file_path_has_parent].
    let parent = path.parent().unwrap().to_owned();

    // Create the directories if necessary and write the file.
    fs::create_dir_all(parent).and_then(move |_| {
        File::create(path).and_then(move |file| {
            tokio::io::write_all(file, payload).and_then(|(file, _)| fsync(file))
        })
    })
}

// Read the state from a file.
pub fn read(state: Arc<RwLock<State>>, path: &Path) -> impl Future<Item = (), Error = Error> {
    // Clone some data that will outlive this function.
    let path = path.to_owned();

    // Do the read.
    fs::read(path).and_then(move |data| {
        // The `unwrap` is safe since it can only fail if a panic already happened.
        let mut state_borrow = state.write().unwrap();

        bincode::deserialize(&data).ok().map_or_else(
            || {
                err(Error::new(
                    InvalidData,
                    "Unable to deserialize the persisted state.",
                ))
            },
            |result| {
                *state_borrow = result;
                ok(())
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::state::ProposalNumber;

    #[test]
    fn proposal_ord_round() {
        let pn0 = ProposalNumber {
            round: 0,
            proposer_ip: 1,
            proposer_port: 1,
        };

        let pn1 = ProposalNumber {
            round: 1,
            proposer_ip: 0,
            proposer_port: 0,
        };

        assert!(pn1 > pn0);
    }

    #[test]
    fn proposal_ord_proposer_ip() {
        let pn0 = ProposalNumber {
            round: 0,
            proposer_ip: 0,
            proposer_port: 1,
        };

        let pn1 = ProposalNumber {
            round: 0,
            proposer_ip: 1,
            proposer_port: 0,
        };

        assert!(pn1 > pn0);
    }

    #[test]
    fn proposal_ord_proposer_port() {
        let pn0 = ProposalNumber {
            round: 0,
            proposer_ip: 0,
            proposer_port: 0,
        };

        let pn1 = ProposalNumber {
            round: 0,
            proposer_ip: 0,
            proposer_port: 1,
        };

        assert!(pn1 > pn0);
    }
}
