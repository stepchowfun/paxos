use {
    crate::{
        acceptor::{
            AcceptRequest, AcceptResponse, ChooseRequest, ChooseResponse, PrepareRequest,
            PrepareResponse, ACCEPT_ENDPOINT, CHOOSE_ENDPOINT, PREPARE_ENDPOINT,
        },
        state::{ProposalNumber, State},
    },
    futures::{stream::FuturesUnordered, StreamExt},
    hyper::{client::HttpConnector, Body, Client, Method, Request},
    rand::{thread_rng, Rng},
    serde::{de::DeserializeOwned, Serialize},
    std::{cmp::min, io, net::SocketAddr, path::Path, sync::Arc, time::Duration},
    tokio::{sync::RwLock, time::sleep},
};

// Duration constants
const EXPONENTIAL_BACKOFF_MIN: Duration = Duration::from_millis(100);
const EXPONENTIAL_BACKOFF_MAX: Duration = Duration::from_secs(2);
const EXPONENTIAL_BACKOFF_MULTIPLIER: u32 = 2;
const RESTART_DELAY_MIN: Duration = Duration::from_millis(0);
const RESTART_DELAY_MAX: Duration = Duration::from_millis(100);

// Generate a new proposal number.
fn generate_proposal_number(
    nodes: &[SocketAddr],
    node_index: usize,
    state: &mut State,
) -> ProposalNumber {
    let proposal_number = ProposalNumber {
        round: state.next_round,
        proposer_address: nodes[node_index],
    };
    state.next_round += 1;
    proposal_number
}

// Send a request without retries.
async fn try_to_send<T: DeserializeOwned>(
    client: &Client<HttpConnector, Body>,
    node: SocketAddr,
    endpoint: &str,
    payload: &impl Serialize,
) -> Result<T, hyper::Error> {
    Ok(bincode::deserialize(
        &hyper::body::to_bytes(
            client
                .request(
                    Request::builder()
                        .method(Method::POST)
                        .uri(format!("http://{}{}", node, endpoint))
                        // The `unwrap` is safe because serialization should never fail.
                        .body(Body::from(bincode::serialize(&payload).unwrap()))
                        .unwrap(), // Safe since we constructed a well-formed request
                )
                .await?
                .into_body(),
        )
        .await?,
    )
    .unwrap()) // Safe under non-Byzantine conditions
}

// Send a request, retrying with exponential backoff until it succeeds.
async fn send<T: DeserializeOwned>(
    client: &Client<HttpConnector, Body>,
    node: SocketAddr,
    endpoint: &str,
    payload: &impl Serialize,
) -> T {
    // The delay between requests
    let mut delay = EXPONENTIAL_BACKOFF_MIN;

    // Retry until the request succeeds.
    loop {
        // Send the request.
        match try_to_send(client, node, endpoint, payload).await {
            Ok(response) => {
                return response;
            }
            Err(error) => {
                // Log the error.
                error!("Received error: {}", error);
            }
        }

        // Sleep before retrying.
        sleep(delay).await;
        delay = min(
            delay * EXPONENTIAL_BACKOFF_MULTIPLIER,
            EXPONENTIAL_BACKOFF_MAX,
        );
    }
}

// Send a request to all nodes. Return once a majority of responses come in.
async fn broadcast_quorum<T: DeserializeOwned>(
    client: &Client<HttpConnector, Body>,
    nodes: &[SocketAddr],
    endpoint: &str,
    payload: &impl Serialize,
) -> Vec<T> {
    nodes
        .iter()
        .map(|node| send(client, *node, endpoint, payload))
        .collect::<FuturesUnordered<_>>()
        .take(nodes.len() / 2 + 1)
        .collect()
        .await
}

// Send a request to all nodes. Return once all responses come in.
async fn broadcast_all<T: DeserializeOwned>(
    client: &Client<HttpConnector, Body>,
    nodes: &[SocketAddr],
    endpoint: &str,
    payload: &impl Serialize,
) -> Vec<T> {
    nodes
        .iter()
        .map(|node| send(client, *node, endpoint, payload))
        .collect::<FuturesUnordered<_>>()
        .collect()
        .await
}

// Propose a value to the cluster.
pub async fn propose(
    state: Arc<RwLock<State>>,
    data_file_path: &Path,
    nodes: &[SocketAddr],
    node_index: usize,
    original_value: &str,
) -> Result<(), io::Error> {
    // Retry until the protocol succeeds.
    loop {
        // Generate a new proposal number.
        let proposal_number = {
            // The `unwrap` is safe since it can only fail if a panic already happened.
            let mut guard = state.write().await;
            generate_proposal_number(nodes, node_index, &mut guard)
        };

        // Persist the state.
        {
            // The `unwrap` is safe since it can only fail if a panic already happened.
            let guard = state.read().await;
            crate::state::write(&guard, data_file_path).await?;
        }

        // Create an HTTP client.
        let client = Client::new();

        // Send a prepare message to all the nodes.
        info!(
            "Preparing value `{}` with proposal number:\n{}",
            original_value,
            // Serialization is safe.
            serde_yaml::to_string(&proposal_number).unwrap(),
        );
        let prepare_responses = broadcast_quorum::<PrepareResponse>(
            &client,
            nodes,
            PREPARE_ENDPOINT,
            &PrepareRequest { proposal_number },
        )
        .await;

        // Determine which value to propose.
        let new_value = if let Some(accepted_proposal) = prepare_responses
            .iter()
            .filter_map(|response| response.accepted_proposal.clone())
            .max_by_key(|accepted_proposal| accepted_proposal.0)
        {
            // There was an accepted proposal. Use that.
            info!(
                "Discovered existing value from cluster: {}",
                accepted_proposal.1,
            );
            accepted_proposal.1
        } else {
            // Propose the given value.
            info!("Quorum replied with no existing value.");
            original_value.to_owned()
        };

        // Send an accept message to all the nodes.
        info!(
            "Requesting acceptance of value `{}` with proposal number:\n{}",
            new_value,
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

        // Was the proposed value chosen?
        if accept_responses
            .iter()
            .all(|response| response.min_proposal_number == proposal_number)
        {
            // The protocol succeeded. Notify all the nodes and return.
            info!("Consensus achieved. Notifying all the nodes.");
            broadcast_all::<ChooseResponse>(
                &client,
                nodes,
                CHOOSE_ENDPOINT,
                &ChooseRequest { value: new_value },
            )
            .await;
            info!("All nodes notified.");
            return Ok(());
        }

        // The protocol failed. Sleep for a random duration before starting over.
        info!("Failed to reach consensus. Starting over.");
        sleep(thread_rng().gen_range(RESTART_DELAY_MIN..RESTART_DELAY_MAX)).await;
    }
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
        let pn = generate_proposal_number(&nodes, 1, &mut state);
        assert_eq!(pn.round, 0);
        assert_eq!(pn.proposer_address, address1);
    }

    #[test]
    fn second_proposal_number() {
        let mut state = initial();
        let nodes = vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            3000,
        )];
        let pn0 = generate_proposal_number(&nodes, 0, &mut state);
        let pn1 = generate_proposal_number(&nodes, 0, &mut state);
        assert!(pn1 > pn0);
    }
}
