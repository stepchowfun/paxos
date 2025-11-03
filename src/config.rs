use {
    serde::{Deserialize, Serialize},
    std::{io, net::SocketAddr, path::Path},
    tokio::{fs::File, io::AsyncReadExt},
};

// A program configuration
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub nodes: Vec<SocketAddr>,
}

// Read the config from a file.
pub async fn read(path: &Path) -> io::Result<Config> {
    // Read the file into a buffer.
    let mut file = File::open(path).await?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;

    // Deserialize the data.
    serde_yaml::from_slice(&contents).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Error loading config file `{}`. Reason: {}",
                path.to_string_lossy(),
                error,
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use {
        crate::config::Config,
        std::net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    #[test]
    fn parse_empty() {
        let config = r"
nodes: []
    "
        .trim();

        let result = Config { nodes: vec![] };

        assert_eq!(serde_yaml::from_str::<Config>(config).unwrap(), result);
    }

    #[test]
    fn parse_single() {
        let config = r#"
nodes:
  - "127.0.0.1:3000"
    "#
        .trim();

        let result = Config {
            nodes: vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000)],
        };

        assert_eq!(serde_yaml::from_str::<Config>(config).unwrap(), result);
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

        let result = Config {
            nodes: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)), 3000),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 2)), 3001),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 3)), 3002),
            ],
        };

        assert_eq!(serde_yaml::from_str::<Config>(config).unwrap(), result);
    }
}
