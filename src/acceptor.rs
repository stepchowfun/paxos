use {
    crate::state::{self, ProposalNumber},
    hyper::{
        header::CONTENT_TYPE,
        server::conn::AddrStream,
        service::{make_service_fn, service_fn},
        Body, Method, Request, Response, Server, StatusCode,
    },
    serde::{Deserialize, Serialize},
    std::{
        convert::Infallible,
        io::{self, Write},
        net::SocketAddr,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tokio::sync::RwLock,
};

// We embed the favicon directly into the compiled binary.
const FAVICON_DATA: &[u8] = include_bytes!("../resources/favicon.ico");

// Endpoints
pub const PREPARE_ENDPOINT: &str = "/prepare";
pub const ACCEPT_ENDPOINT: &str = "/accept";
pub const CHOOSE_ENDPOINT: &str = "/choose";

// Request type for the "prepare" endpoint
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
    pub proposal_number: Option<ProposalNumber>,
}

// Response type for the "prepare" endpoint
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareResponse {
    pub accepted_proposal: Option<(ProposalNumber, String)>,
}

// Logic for the "prepare" endpoint
fn prepare(
    request: &PrepareRequest,
    state: &mut (state::Durable, state::Volatile),
) -> PrepareResponse {
    debug!(
        "Received prepare request:\n{}",
        serde_yaml::to_string(request).unwrap(), // Serialization is safe.
    );

    if let Some(requested_proposal_number) = request.proposal_number {
        match &state.0.min_proposal_number {
            Some(proposal_number) => {
                if requested_proposal_number > *proposal_number {
                    state.0.min_proposal_number = Some(requested_proposal_number);
                }
            }
            None => {
                state.0.min_proposal_number = Some(requested_proposal_number);
            }
        }
    }

    PrepareResponse {
        accepted_proposal: state.0.accepted_proposal.clone(),
    }
}

// Request type for the "accept" endpoint
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptRequest {
    pub proposal: (ProposalNumber, String),
}

// Response type for the "accept" endpoint
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptResponse {
    pub min_proposal_number: ProposalNumber,
}

// Logic for the "accept" endpoint
fn accept(
    request: &AcceptRequest,
    state: &mut (state::Durable, state::Volatile),
) -> AcceptResponse {
    debug!(
        "Received accept request:\n{}",
        serde_yaml::to_string(request).unwrap(), // Serialization is safe.
    );

    if state
        .0
        .min_proposal_number
        .as_ref()
        .map_or(true, |proposal_number| {
            request.proposal.0 >= *proposal_number
        })
    {
        state.0.min_proposal_number = Some(request.proposal.0);
        state.0.accepted_proposal = Some(request.proposal.clone());
    }

    AcceptResponse {
        // The `unwrap` is safe since accepts must follow at least one prepare.
        min_proposal_number: state.0.min_proposal_number.unwrap(),
    }
}

// Request type for the "choose" endpoint
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseRequest {
    pub value: String,
}

// Response type for the "choose" endpoint
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseResponse;

// Logic for the "choose" endpoint
fn choose(
    request: &ChooseRequest,
    state: &mut (state::Durable, state::Volatile),
) -> ChooseResponse {
    if state.1.chosen_value.is_none() {
        info!("Consensus achieved.");
        println!("{}", request.value);
        io::stdout().flush().unwrap_or(());
        state.1.chosen_value = Some(request.value.clone());
    }
    ChooseResponse {}
}

// Context for each service instance
#[derive(Clone)]
struct Context {
    state: Arc<RwLock<(state::Durable, state::Volatile)>>,
    data_file_path: PathBuf,
}

// Request handler
async fn handle_request(
    context: Context,
    request: Request<Body>,
) -> Result<Response<Body>, io::Error> {
    // This macro eliminates some boilerplate in the match expression below.
    macro_rules! rpc {
        ($endpoint:ident) => {{
            // Collect the body into a byte array.
            let body = hyper::body::to_bytes(request.into_body())
                .await
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Unable to read request body. Reason: {}", error),
                    )
                })?;

            // Parse the body.
            let payload = bincode::deserialize(&body).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unable to parse request body. Reason: {}", error),
                )
            })?;

            // Handle the request.
            let mut guard = context.state.write().await;
            let response = $endpoint(&payload, &mut guard);
            crate::state::write(&guard.0, &context.data_file_path).await?;

            // Serialize the response.
            Ok(Response::new(Body::from(
                bincode::serialize(&response).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Unable to serialize response. Reason: {}", error),
                    )
                })?,
            )))
        }};
    }

    // Match on the route and handle the request appropriately.
    match (request.method(), request.uri().path()) {
        // RPC calls
        (&Method::POST, PREPARE_ENDPOINT) => rpc![prepare],
        (&Method::POST, ACCEPT_ENDPOINT) => rpc![accept],
        (&Method::POST, CHOOSE_ENDPOINT) => rpc![choose],

        // Summary of the program state
        (&Method::GET, "/") => {
            // Respond with a representation of the program state. The `unwrap`s
            // are safe because serialization should never fail.
            let state = context.state.read().await;
            let durable_state_repr = serde_yaml::to_string(&state.0).unwrap();
            let volatile_state_repr = serde_yaml::to_string(&state.1).unwrap();
            Ok(Response::new(Body::from(format!(
                "System operational.\n\n\
                Durable state:\n\n\
                {durable_state_repr}\n\n\
                Volatile state:\n\n\
                {volatile_state_repr}",
            ))))
        }

        // Favicon
        (&Method::GET, "/favicon.ico") => {
            // Respond with the favicon.
            Ok(Response::builder()
                .header(CONTENT_TYPE, "image/x-icon")
                .body(Body::from(FAVICON_DATA))
                // The `unwrap` is safe since we constructed a well-formed
                // response.
                .unwrap())
        }

        // Catch-all
        _ => {
            // Respond with a generic 404 page.
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not found."))
                // The `unwrap` is safe since we constructed a well-formed
                // response.
                .unwrap())
        }
    }
}

// Entrypoint for the acceptor
pub async fn acceptor(
    state: Arc<RwLock<(state::Durable, state::Volatile)>>,
    data_file_path: &Path,
    address: SocketAddr,
) -> Result<(), io::Error> {
    // Set up the HTTP server for the acceptor.
    let context = Context {
        state,
        data_file_path: data_file_path.to_owned(),
    };
    let server = Server::bind(&address).serve(make_service_fn(move |_connection: &AddrStream| {
        let context = context.clone();
        let service = service_fn(move |request| handle_request(context.clone(), request));
        async move { Ok::<_, Infallible>(service) }
    }));

    // Tell the user the address of the server.
    info!("Listening on http://{}/", address);

    // Wait on the server.
    server.await.map_err(|error| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Server failed. Reason: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use {
        crate::{
            acceptor::{accept, choose, prepare, AcceptRequest, ChooseRequest, PrepareRequest},
            state::{initial, ProposalNumber},
        },
        std::net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    #[test]
    fn prepare_initializes_min_proposal_number() {
        let mut state = initial();
        let request = PrepareRequest {
            proposal_number: Some(ProposalNumber {
                round: 0,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            }),
        };
        let response = prepare(&request, &mut state);
        assert_eq!(state.0.min_proposal_number, request.proposal_number);
        assert_eq!(response.accepted_proposal, None);
    }

    #[test]
    fn prepare_increases_min_proposal_number() {
        let mut state = initial();
        state.0.min_proposal_number = Some(ProposalNumber {
            round: 0,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        });
        let request = PrepareRequest {
            proposal_number: Some(ProposalNumber {
                round: 1,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            }),
        };
        let response = prepare(&request, &mut state);
        assert_eq!(state.0.min_proposal_number, request.proposal_number);
        assert_eq!(response.accepted_proposal, None);
    }

    #[test]
    fn prepare_does_not_decrease_min_proposal_number() {
        let mut state = initial();
        state.0.min_proposal_number = Some(ProposalNumber {
            round: 1,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        });
        let request = PrepareRequest {
            proposal_number: Some(ProposalNumber {
                round: 0,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            }),
        };
        let response = prepare(&request, &mut state);
        assert_ne!(state.0.min_proposal_number, request.proposal_number);
        assert_eq!(response.accepted_proposal, None);
    }

    #[test]
    fn prepare_returns_accepted_proposal() {
        let mut state = initial();
        let accepted_proposal = (
            ProposalNumber {
                round: 0,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            },
            "foo".to_string(),
        );
        state.0.min_proposal_number = Some(accepted_proposal.0);
        state.0.accepted_proposal = Some(accepted_proposal.clone());
        let request = PrepareRequest {
            proposal_number: Some(ProposalNumber {
                round: 1,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            }),
        };
        let response = prepare(&request, &mut state);
        assert_eq!(response.accepted_proposal, Some(accepted_proposal));
    }

    #[test]
    fn accept_success() {
        let mut state = initial();
        let proposal = (
            ProposalNumber {
                round: 0,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            },
            "foo".to_string(),
        );

        let prepare_request = PrepareRequest {
            proposal_number: Some(proposal.0),
        };
        prepare(&prepare_request, &mut state);

        let accept_request = AcceptRequest {
            proposal: proposal.clone(),
        };
        let accept_response = accept(&accept_request, &mut state);

        assert_eq!(state.0.accepted_proposal, Some(proposal.clone()));
        assert_eq!(accept_response.min_proposal_number, proposal.0);
        assert_eq!(state.0.min_proposal_number, Some(proposal.0));
    }

    #[test]
    fn accept_failure() {
        let mut state = initial();
        let proposal0 = (
            ProposalNumber {
                round: 0,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            },
            "foo".to_string(),
        );

        let proposal1 = (
            ProposalNumber {
                round: 1,
                proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081),
            },
            "bar".to_string(),
        );

        let prepare_request1 = PrepareRequest {
            proposal_number: Some(proposal0.0),
        };
        prepare(&prepare_request1, &mut state);

        let prepare_request2 = PrepareRequest {
            proposal_number: Some(proposal1.0),
        };
        prepare(&prepare_request2, &mut state);

        let accept_request = AcceptRequest {
            proposal: proposal0,
        };
        let accept_response = accept(&accept_request, &mut state);

        assert_eq!(state.0.accepted_proposal, None);
        assert_eq!(accept_response.min_proposal_number, proposal1.0);
        assert_eq!(state.0.min_proposal_number, Some(proposal1.0));
    }

    #[test]
    fn choose_updates_state() {
        let mut state = initial();
        let request = ChooseRequest {
            value: "foo".to_string(),
        };
        choose(&request, &mut state);
        assert_eq!(state.1.chosen_value, Some(request.value));
    }
}
