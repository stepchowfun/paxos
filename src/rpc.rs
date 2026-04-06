use {
    bytes::Bytes,
    futures::{StreamExt, stream::FuturesUnordered},
    http_body_util::{BodyExt, Full},
    hyper::{Method, Request},
    hyper_util::{
        client::legacy::{Client, connect::HttpConnector},
        rt::TokioExecutor,
    },
    serde::{Serialize, de::DeserializeOwned},
    std::{cmp::min, io, net::SocketAddr},
    tokio::time::{Duration, sleep},
};

// Duration constants
const EXPONENTIAL_BACKOFF_MIN: Duration = Duration::from_millis(50);
const EXPONENTIAL_BACKOFF_MAX: Duration = Duration::from_secs(1);
const EXPONENTIAL_BACKOFF_MULTIPLIER: u32 = 2;

pub type HttpClient = Client<HttpConnector, Full<Bytes>>;

// Create an HTTP client for Paxos RPC requests.
pub fn new_client() -> HttpClient {
    Client::builder(TokioExecutor::new()).build(HttpConnector::new())
}

// Send a request without retries.
async fn try_to_send<T: DeserializeOwned>(
    client: &HttpClient,
    node: SocketAddr,
    endpoint: &str,
    payload: &impl Serialize,
) -> io::Result<T> {
    let response = client
        .request(
            Request::builder()
                .method(Method::POST)
                .uri(format!("http://{node}{endpoint}"))
                // The `unwrap` is safe because serialization should never fail.
                .body(Full::new(Bytes::from(serde_json::to_vec(payload).unwrap())))
                .unwrap(), // Safe since we constructed a well-formed request
        )
        .await
        .map_err(|error| io::Error::other(format!("Unable to send request. Reason: {error}")))?;

    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|error| {
            io::Error::other(format!("Unable to read response body. Reason: {error}"))
        })?
        .to_bytes();

    serde_json::from_slice(&body).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unable to parse response body. Reason: {error}"),
        )
    })
}

// Send a request, retrying with exponential backoff until it succeeds.
async fn send<T: DeserializeOwned>(
    client: &HttpClient,
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
                debug!("Received error: {error}");
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
    client: &HttpClient,
    nodes: &[SocketAddr],
    endpoint: &str,
    payload: &impl Serialize,
) -> Vec<Result<T, io::Error>> {
    nodes
        .iter()
        .map(|node| try_to_send(client, *node, endpoint, payload))
        .collect::<FuturesUnordered<_>>()
        .collect()
        .await
}

// Send a request to all nodes with retries. Return once a majority of responses come in.
pub async fn broadcast_quorum<T: DeserializeOwned>(
    client: &HttpClient,
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
