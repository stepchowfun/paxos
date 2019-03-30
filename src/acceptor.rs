use crate::protocol::{ProposalNumber, State};
use serde::{Deserialize, Serialize};

// Endpoints
pub const PREPARE_ENDPOINT: &str = "/prepare";
pub const ACCEPT_ENDPOINT: &str = "/accept";
pub const CHOOSE_ENDPOINT: &str = "/choose";

// BEGIN PREPARE

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

pub fn prepare(
  request: &PrepareRequest,
  state: &mut State,
) -> PrepareResponse {
  info!(
    "Received prepare message:\n{}",
    serde_yaml::to_string(request).unwrap() // Serialization is safe.
  );

  match &state.min_proposal_number {
    Some(proposal_number) => {
      if request.proposal_number > *proposal_number {
        state.min_proposal_number = Some(request.proposal_number.clone());
      }
    }
    None => {
      state.min_proposal_number = Some(request.proposal_number.clone());
    }
  }

  PrepareResponse {
    accepted_proposal: state.accepted_proposal.clone(),
  }
}

// END PREPARE

// BEGIN ACCEPT

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

pub fn accept(request: &AcceptRequest, state: &mut State) -> AcceptResponse {
  info!(
    "Received accept message:\n{}",
    serde_yaml::to_string(request).unwrap() // Serialization is safe.
  );
  match &state.min_proposal_number {
    Some(proposal_number) => {
      if request.proposal.0 >= *proposal_number {
        state.min_proposal_number = Some(request.proposal.0.clone());
        state.accepted_proposal = Some(request.proposal.clone());
      }
    }
    None => {
      state.min_proposal_number = Some(request.proposal.0.clone());
      state.accepted_proposal = Some(request.proposal.clone());
    }
  }

  AcceptResponse {
     // The `unwrap` is safe since accepts must follow at least one prepare.
    min_proposal_number: state.min_proposal_number.clone().unwrap(),
  }
}

// END ACCEPT

// BEGIN CHOOSE

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseRequest {
  pub value: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseResponse;

pub fn choose(request: &ChooseRequest, state: &mut State) -> ChooseResponse {
  info!("Consensus achieved.");
  if state.chosen_value.is_none() {
    println!("{}", request.value);
    state.chosen_value = Some(request.value.clone());
  }
  ChooseResponse {}
}

// END CHOOSE

#[cfg(test)]
mod tests {}
