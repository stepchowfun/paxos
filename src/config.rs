use serde::{Deserialize, Serialize};
use std::net::SocketAddrV4;

// A program configuration
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub nodes: Vec<SocketAddrV4>,
}

// Parse config data.
pub fn parse(config: &str) -> Result<Config, String> {
    serde_yaml::from_str(config).map_err(|e| format!("{}", e))
}

#[cfg(test)]
mod tests {
    use crate::config::{parse, Config};
    use std::net::{Ipv4Addr, SocketAddrV4};

    #[test]
    fn parse_empty() {
        let config = r#"
nodes: []
    "#
        .trim();

        let result = Ok(Config { nodes: vec![] });

        assert_eq!(parse(config), result);
    }

    #[test]
    fn parse_single() {
        let config = r#"
nodes:
  - "127.0.0.1:3000"
    "#
        .trim();

        let result = Ok(Config {
            nodes: vec![SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3000)],
        });

        assert_eq!(parse(config), result);
    }

    #[test]
    fn parse_multiple() {
        let config = r#"
nodes:
  - "192.168.0.1:3000"
  - "192.168.0.2:3001"
  - "192.168.0.3:3002"
    "#
        .trim();

        let result = Ok(Config {
            nodes: vec![
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 3000),
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 2), 3001),
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 3), 3002),
            ],
        });

        assert_eq!(parse(config), result);
    }
}
