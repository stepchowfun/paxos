use crate::acceptor::{
    AcceptRequest, AcceptResponse, ChooseRequest, ChooseResponse, PrepareRequest, PrepareResponse,
    ACCEPT_ENDPOINT, CHOOSE_ENDPOINT, PREPARE_ENDPOINT,
};
use crate::state::{ProposalNumber, State};
use crate::util::{broadcast, when};
use futures::{future::ok, prelude::*};
use hyper::{client::HttpConnector, Client};
use rand::{thread_rng, Rng};
use std::{
    net::SocketAddrV4,
    path::Path,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::timer::Delay;

// Duration constants
const RESTART_DELAY_MIN: Duration = Duration::from_millis(0);
const RESTART_DELAY_MAX: Duration = Duration::from_millis(100);

// This function generates a new proposal number.
fn generate_proposal_number(
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

pub fn propose(
    client: &Client<HttpConnector>,
    nodes: &[SocketAddrV4],
    node_index: usize,
    state: Arc<RwLock<State>>,
    data_file_path: &Path,
    value: &str,
) -> impl Future<Item = (), Error = ()> + Send {
    // Clone some data that will outlive this function.
    let client = client.clone();
    let nodes = nodes.to_owned();
    let data_file_path = data_file_path.to_owned();
    let value = value.to_owned();

    // Generate a new proposal number.
    let proposal_number = {
        // The `unwrap` is safe since it can only fail if a panic already happened.
        let mut state_borrow = state.write().unwrap();
        generate_proposal_number(&nodes, node_index, &mut state_borrow)
    };

    // Persist the state.
    {
        // The `unwrap` is safe since it can only fail if a panic already happened.
        let state_borrow = state.read().unwrap();

        crate::state::write(&state_borrow, &data_file_path)
    }
    .map_err(|e| {
        error!("{}", e);
    })
    .and_then(move |_| {
        // Clone some data that will outlive this function.
        let value_for_prepare = value.clone();

        // Send a prepare message to all the nodes.
        info!(
            "Preparing value `{}` with proposal number:\n{}",
            value,
            // Serialization is safe.
            serde_yaml::to_string(&proposal_number).unwrap()
        );
        let prepares = broadcast(
            &nodes,
            &client,
            PREPARE_ENDPOINT,
            PrepareRequest { proposal_number },
        );

        // Wait for a majority of the nodes to respond.
        let majority = nodes.len() / 2 + 1;
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
                    .max_by_key(|accepted_proposal| accepted_proposal.0);
                if let Some(proposal) = accepted_proposal {
                    // There was an existing proposal. Use that.
                    info!("Discovered existing value from quorum: {}", proposal.1);
                    Some(proposal.1)
                } else {
                    // Propose the given value.
                    info!("Quorum replied with no existing value.");
                    Some(value_for_prepare.clone())
                }
            }
        })
        .and_then(move |value_for_accept| {
            // Send an accept message to all the nodes.
            info!(
                "Requesting acceptance of value `{}` with proposal number:\n{}",
                value_for_accept,
                // The `unwrap` is safe because serialization should never fail.
                serde_yaml::to_string(&proposal_number).unwrap()
            );
            let accepts = broadcast(
                &nodes,
                &client,
                ACCEPT_ENDPOINT,
                AcceptRequest {
                    proposal: (proposal_number, value_for_accept.clone()),
                },
            );

            // Wait for a majority of the nodes to respond.
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
                    info!("Consensus achieved. Notifying all the nodes.");
                    Box::new(
                        broadcast(
                            &nodes,
                            &client,
                            CHOOSE_ENDPOINT,
                            ChooseRequest {
                                value: value_for_accept,
                            },
                        )
                        .fold((), |_, _: ChooseResponse| ok(()))
                        .map(|_| info!("All nodes notified.")),
                    ) as Box<Future<Item = (), Error = ()> + Send>
                } else {
                    // Paxos failed. Start over.
                    info!("Failed to reach consensus. Starting over.");
                    Box::new(
                        Delay::new(
                            Instant::now()
                                + thread_rng().gen_range(RESTART_DELAY_MIN, RESTART_DELAY_MAX),
                        )
                        .map_err(|_| ())
                        .and_then(move |_| {
                            propose(&client, &nodes, node_index, state, &data_file_path, &value)
                        }),
                    ) as Box<Future<Item = (), Error = ()> + Send>
                }
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use crate::proposer::generate_proposal_number;
    use crate::state::initial;
    use std::net::{Ipv4Addr, SocketAddrV4};

    #[test]
    fn first_proposal_number() {
        let mut state = initial();
        let address0 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000);
        let address1 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 2), 3001);
        let address2 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 3), 3002);
        let nodes = vec![address0, address1, address2];
        let pn = generate_proposal_number(&nodes, 1, &mut state);
        assert_eq!(pn.round, 0);
        assert_eq!(pn.proposer_ip, u32::from(*address1.ip()));
        assert_eq!(pn.proposer_port, address1.port());
    }

    #[test]
    fn second_proposal_number() {
        let mut state = initial();
        let nodes = vec![SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000)];
        let pn0 = generate_proposal_number(&nodes, 0, &mut state);
        let pn1 = generate_proposal_number(&nodes, 0, &mut state);
        assert!(pn1 > pn0);
    }
}
