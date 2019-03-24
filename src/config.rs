use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
  pub address: SocketAddr,
  #[serde(skip_deserializing)]
  pub me: bool,
}

// Parse config data.
pub fn parse(config: &str) -> Result<Vec<Node>, String> {
  serde_yaml::from_str(config).map_err(|err| format!("{}", err))
}
