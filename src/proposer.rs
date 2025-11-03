use {
    crate::{
        acceptor::{
            ACCEPT_ENDPOINT, AcceptRequest, AcceptResponse, CHOOSE_ENDPOINT, ChooseRequest,
            ChooseResponse, PREPARE_ENDPOINT, PrepareRequest, PrepareResponse,
        },
        rpc::{broadcast_quorum, try_to_broadcast},
        state::{self, ProposalNumber},
    },
    hyper::Client,
    std::{io, net::SocketAddr, path::Path, sync::Arc},
    tokio::sync::RwLock,
};

// Generate a new proposal number.
fn generate_proposal_number(
    nodes: &[SocketAddr],
    node_index: usize,
    state: &mut state::Durable,
) -> ProposalNumber {
    let proposal_number = ProposalNumber {
        round: state.next_round,
        proposer_address: nodes[node_index],
    };
    state.next_round += 1;
    proposal_number
}

// Propose a value to the cluster.
pub async fn propose(
    state: Arc<RwLock<(state::Durable, state::Volatile)>>,
    data_file_path: &Path,
    nodes: &[SocketAddr],
    node_index: usize,
    original_value: Option<&str>,
) -> Result<(), io::Error> {
    // Create an HTTP client.
    let client = Client::new();

    // Retry until the protocol succeeds.
    loop {
        // Generate a new proposal number.
        let proposal_number = {
            // The `unwrap` is safe since it can only fail if a panic already happened.
            let mut guard = state.write().await;
            let proposal_number = generate_proposal_number(nodes, node_index, &mut guard.0);
            crate::state::write(&guard.0, data_file_path).await?;
            proposal_number
        };

        // Send a prepare message to all the nodes.
        debug!(
            "Preparing proposal number:\n{}",
            // Serialization is safe.
            serde_yaml::to_string(&proposal_number).unwrap(),
        );
        let prepare_responses = broadcast_quorum::<PrepareResponse>(
            &client,
            nodes,
            PREPARE_ENDPOINT,
            &PrepareRequest {
                proposal_number: Some(proposal_number),
            },
        )
        .await;

        // Determine which value to propose.
        let new_value = if let Some(accepted_proposal) = prepare_responses
            .iter()
            .filter_map(|response| response.accepted_proposal.clone())
            .max_by_key(|accepted_proposal| accepted_proposal.0)
        {
            // There was an accepted proposal. Use that.
            debug!(
                "Discovered existing value from cluster: {}",
                accepted_proposal.1,
            );
            accepted_proposal.1
        } else {
            // Propose the given value, or break if there isn't one.
            if let Some(original_value) = original_value {
                debug!("Quorum replied with no existing value.");
                original_value.to_owned()
            } else {
                break;
            }
        };

        // Send an accept message to all the nodes.
        debug!(
            "Requesting acceptance of value `{}`.",
            // The `unwrap` is safe because serialization should never fail.
            serde_yaml::to_string(&proposal_number).unwrap(),
        );
        let accept_responses = broadcast_quorum::<AcceptResponse>(
            &client,
            nodes,
            ACCEPT_ENDPOINT,
            &AcceptRequest {
                proposal: (proposal_number, new_value.clone()),
            },
        )
        .await;

        // Determine if the proposed value was chosen.
        let mut value_chosen = true;
        for response in accept_responses {
            if response.min_proposal_number > proposal_number {
                value_chosen = false;
            }

            // Update the `next_round`, if applicable. The `unwrap` is safe
            // since it can only fail if a panic already happened.
            let mut guard = state.write().await;
            if guard.0.next_round <= response.min_proposal_number.round {
                guard.0.next_round = response.min_proposal_number.round + 1;
                crate::state::write(&guard.0, data_file_path).await?;
            }
        }
        if value_chosen {
            // The protocol succeeded. Notify all the nodes and return.
            debug!("Consensus achieved. Notifying all the nodes.");
            try_to_broadcast::<ChooseResponse>(
                &client,
                nodes,
                CHOOSE_ENDPOINT,
                &ChooseRequest { value: new_value },
            )
            .await;
            debug!("Proposer finished.");
            return Ok(());
        }

        // The protocol failed. Sleep for a random duration before starting over.
        debug!("Failed to reach consensus. Starting over.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        crate::{proposer::generate_proposal_number, state::initial},
        std::net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    #[test]
    fn first_proposal_number() {
        let mut state = initial();
        let address0 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)), 3000);
        let address1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 3001);
        let address2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3)), 3002);
        let nodes = vec![address0, address1, address2];
        let pn = generate_proposal_number(&nodes, 1, &mut state.0);
        assert_eq!(pn.round, 0);
        assert_eq!(pn.proposer_address, address1);
    }

    #[test]
    fn second_proposal_number() {
        let mut state = initial();
        let nodes = vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000)];
        let pn0 = generate_proposal_number(&nodes, 0, &mut state.0);
        let pn1 = generate_proposal_number(&nodes, 0, &mut state.0);
        assert!(pn1 > pn0);
    }
}
