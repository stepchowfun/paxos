use {
    futures::{stream::FuturesUnordered, StreamExt},
    hyper::{client::HttpConnector, Body, Client, Method, Request},
    serde::{de::DeserializeOwned, Serialize},
    std::{cmp::min, net::SocketAddr},
    tokio::time::{sleep, Duration},
};

// Duration constants
const EXPONENTIAL_BACKOFF_MIN: Duration = Duration::from_millis(50);
const EXPONENTIAL_BACKOFF_MAX: Duration = Duration::from_secs(1);
const EXPONENTIAL_BACKOFF_MULTIPLIER: u32 = 2;

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
                        .uri(format!("http://{node}{endpoint}"))
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
                debug!("Received error: {}", error);
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

// Send a request to all nodes without retries. Return once all responses come in.
pub async fn try_to_broadcast<T: DeserializeOwned>(
    client: &Client<HttpConnector, Body>,
    nodes: &[SocketAddr],
    endpoint: &str,
    payload: &impl Serialize,
) -> Vec<Result<T, hyper::Error>> {
    nodes
        .iter()
        .map(|node| try_to_send(client, *node, endpoint, payload))
        .collect::<FuturesUnordered<_>>()
        .collect()
        .await
}

// Send a request to all nodes with retries. Return once a majority of responses come in.
pub async fn broadcast_quorum<T: DeserializeOwned>(
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
