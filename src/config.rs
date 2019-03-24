use serde::{Deserialize, Serialize};
use std::net::SocketAddrV4;

// A node in the network
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
  pub address: SocketAddrV4,
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
  use std::net::{Ipv4Addr, SocketAddrV4};

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
- address: "127.0.0.1:3000"
    "#
    .trim();

    let result = Ok(vec![Node {
      address: SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000),
      me: false,
    }]);

    assert_eq!(parse(config), result);
  }

  #[test]
  fn parse_multiple() {
    let config = r#"
- address: "192.168.0.1:3000"
- address: "192.168.0.2:3001"
- address: "192.168.0.3:3002"
    "#
    .trim();

    let result = Ok(vec![
      Node {
        address: SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 3000),
        me: false,
      },
      Node {
        address: SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 2), 3001),
        me: false,
      },
      Node {
        address: SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 3), 3002),
        me: false,
      },
    ]);

    assert_eq!(parse(config), result);
  }
}
