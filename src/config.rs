use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[cfg(test)]
mod tests {
  use crate::config::{parse, Node};
  use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

  #[test]
  fn parse_empty() {
    let config = r#"
[]
    "#
    .trim();

    let result = Ok(vec![]);

    assert_eq!(parse(config), result);
  }

  #[test]
  fn parse_single() {
    let config = r#"
- address: "0.0.0.0:3000"
    "#
    .trim();

    let result = Ok(vec![Node {
      address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 3000)),
      me: false,
    }]);

    assert_eq!(parse(config), result);
  }

  #[test]
  fn parse_multiple() {
    let config = r#"
- address: "0.0.0.0:3000"
- address: "0.0.0.0:3001"
- address: "0.0.0.0:3002"
    "#
    .trim();

    let result = Ok(vec![
      Node {
        address: SocketAddr::V4(SocketAddrV4::new(
          Ipv4Addr::UNSPECIFIED,
          3000,
        )),
        me: false,
      },
      Node {
        address: SocketAddr::V4(SocketAddrV4::new(
          Ipv4Addr::UNSPECIFIED,
          3001,
        )),
        me: false,
      },
      Node {
        address: SocketAddr::V4(SocketAddrV4::new(
          Ipv4Addr::UNSPECIFIED,
          3002,
        )),
        me: false,
      },
    ]);

    assert_eq!(parse(config), result);
  }
}
