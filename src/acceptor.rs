use crate::state::{ProposalNumber, State};
use serde::{Deserialize, Serialize};

// Endpoints
pub const PREPARE_ENDPOINT: &str = "/prepare";
pub const ACCEPT_ENDPOINT: &str = "/accept";
pub const CHOOSE_ENDPOINT: &str = "/choose";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
    pub proposal_number: ProposalNumber,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareResponse {
    pub accepted_proposal: Option<(ProposalNumber, String)>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptRequest {
    pub proposal: (ProposalNumber, String),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptResponse {
    pub min_proposal_number: ProposalNumber,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseRequest {
    pub value: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseResponse;

pub fn prepare(request: &PrepareRequest, state: &mut State) -> PrepareResponse {
    info!(
        "Received prepare message:\n{}",
        serde_yaml::to_string(request).unwrap() // Serialization is safe.
    );

    match &state.min_proposal_number {
        Some(proposal_number) => {
            if request.proposal_number > *proposal_number {
                state.min_proposal_number = Some(request.proposal_number);
            }
        }
        None => {
            state.min_proposal_number = Some(request.proposal_number);
        }
    }

    PrepareResponse {
        accepted_proposal: state.accepted_proposal.clone(),
    }
}

pub fn accept(request: &AcceptRequest, state: &mut State) -> AcceptResponse {
    info!(
        "Received accept message:\n{}",
        serde_yaml::to_string(request).unwrap() // Serialization is safe.
    );
    if state
        .min_proposal_number
        .as_ref()
        .map_or(true, |x| request.proposal.0 >= *x)
    {
        state.min_proposal_number = Some(request.proposal.0);
        state.accepted_proposal = Some(request.proposal.clone());
    }

    AcceptResponse {
        // The `unwrap` is safe since accepts must follow at least one prepare.
        min_proposal_number: state.min_proposal_number.unwrap(),
    }
}

pub fn choose(request: &ChooseRequest, state: &mut State) -> ChooseResponse {
    info!("Consensus achieved.");
    if state.chosen_value.is_none() {
        println!("{}", request.value);
        state.chosen_value = Some(request.value.clone());
    }
    ChooseResponse {}
}

#[cfg(test)]
mod tests {
    use crate::acceptor::{accept, choose, prepare, AcceptRequest, ChooseRequest, PrepareRequest};
    use crate::state::{initial, ProposalNumber};

    #[test]
    fn prepare_initializes_min_proposal_number() {
        let mut state = initial();
        let request = PrepareRequest {
            proposal_number: ProposalNumber {
                round: 0,
                proposer_ip: 0,
                proposer_port: 0,
            },
        };
        let response = prepare(&request, &mut state);
        assert_eq!(state.min_proposal_number, Some(request.proposal_number));
        assert_eq!(response.accepted_proposal, None);
    }

    #[test]
    fn prepare_increases_min_proposal_number() {
        let mut state = initial();
        state.min_proposal_number = Some(ProposalNumber {
            round: 0,
            proposer_ip: 0,
            proposer_port: 0,
        });
        let request = PrepareRequest {
            proposal_number: ProposalNumber {
                round: 1,
                proposer_ip: 0,
                proposer_port: 0,
            },
        };
        let response = prepare(&request, &mut state);
        assert_eq!(state.min_proposal_number, Some(request.proposal_number));
        assert_eq!(response.accepted_proposal, None);
    }

    #[test]
    fn prepare_does_not_decrease_min_proposal_number() {
        let mut state = initial();
        state.min_proposal_number = Some(ProposalNumber {
            round: 1,
            proposer_ip: 0,
            proposer_port: 0,
        });
        let request = PrepareRequest {
            proposal_number: ProposalNumber {
                round: 0,
                proposer_ip: 0,
                proposer_port: 0,
            },
        };
        let response = prepare(&request, &mut state);
        assert_ne!(state.min_proposal_number, Some(request.proposal_number));
        assert_eq!(response.accepted_proposal, None);
    }

    #[test]
    fn prepare_returns_accepted_proposal() {
        let mut state = initial();
        let accepted_proposal = (
            ProposalNumber {
                round: 0,
                proposer_ip: 0,
                proposer_port: 0,
            },
            "foo".to_string(),
        );
        state.min_proposal_number = Some(accepted_proposal.0);
        state.accepted_proposal = Some(accepted_proposal.clone());
        let request = PrepareRequest {
            proposal_number: ProposalNumber {
                round: 1,
                proposer_ip: 0,
                proposer_port: 0,
            },
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
                proposer_ip: 0,
                proposer_port: 0,
            },
            "foo".to_string(),
        );

        let prepare_request = PrepareRequest {
            proposal_number: proposal.0,
        };
        prepare(&prepare_request, &mut state);

        let accept_request = AcceptRequest {
            proposal: proposal.clone(),
        };
        let accept_response = accept(&accept_request, &mut state);

        assert_eq!(state.accepted_proposal, Some(proposal.clone()));
        assert_eq!(accept_response.min_proposal_number, proposal.0);
        assert_eq!(state.min_proposal_number, Some(proposal.0));
    }

    #[test]
    fn accept_failure() {
        let mut state = initial();
        let proposal0 = (
            ProposalNumber {
                round: 0,
                proposer_ip: 0,
                proposer_port: 0,
            },
            "foo".to_string(),
        );

        let proposal1 = (
            ProposalNumber {
                round: 1,
                proposer_ip: 1,
                proposer_port: 1,
            },
            "bar".to_string(),
        );

        let prepare_request1 = PrepareRequest {
            proposal_number: proposal0.0,
        };
        prepare(&prepare_request1, &mut state);

        let prepare_request2 = PrepareRequest {
            proposal_number: proposal1.0,
        };
        prepare(&prepare_request2, &mut state);

        let accept_request = AcceptRequest {
            proposal: proposal0,
        };
        let accept_response = accept(&accept_request, &mut state);

        assert_eq!(state.accepted_proposal, None);
        assert_eq!(accept_response.min_proposal_number, proposal1.0);
        assert_eq!(state.min_proposal_number, Some(proposal1.0));
    }

    #[test]
    fn choose_updates_state() {
        let mut state = initial();
        let request = ChooseRequest {
            value: "foo".to_string(),
        };
        choose(&request, &mut state);
        assert_eq!(state.chosen_value, Some(request.value));
    }
}
