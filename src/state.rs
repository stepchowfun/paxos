use {
    serde::{Deserialize, Serialize},
    std::{cmp::Ordering, io, net::SocketAddr, path::Path},
    tokio::{
        fs::{File, create_dir_all},
        io::{AsyncReadExt, AsyncWriteExt},
    },
};

// A representation of a proposal number
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalNumber {
    pub round: u64,
    pub proposer_address: SocketAddr,
}

// We implement a custom ordering to ensure that round number takes precedence over proposer.
impl Ord for ProposalNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.round == other.round {
            self.proposer_address.cmp(&other.proposer_address)
        } else {
            self.round.cmp(&other.round)
        }
    }
}

// `Ord` requires `PartialOrd`.
impl PartialOrd for ProposalNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// The part of the program's state that needs to be persisted
#[derive(Deserialize, Serialize)]
pub struct Durable {
    pub next_round: u64,
    pub min_proposal_number: Option<ProposalNumber>,
    pub accepted_proposal: Option<(ProposalNumber, String)>,
}

// The part of the program's state that doesn't need to be persisted
#[derive(Serialize)]
pub struct Volatile {
    pub chosen_value: Option<String>,
}

// Return the state in which the program starts.
pub fn initial() -> (Durable, Volatile) {
    (
        Durable {
            next_round: 0,
            min_proposal_number: None,
            accepted_proposal: None,
        },
        Volatile { chosen_value: None },
    )
}

// Write the state to a file.
pub async fn write(state: &Durable, path: &Path) -> io::Result<()> {
    // The `unwrap` is safe because serialization should never fail.
    let payload = bincode::serialize(&state).unwrap();

    // The `unwrap` is safe due to [ref:data_file_path_has_parent].
    let parent = path.parent().unwrap().to_owned();

    // Create the directories if necessary and write the file.
    create_dir_all(parent).await?;
    let mut file = File::create(path).await?;
    file.write_all(&payload).await?;
    file.sync_all().await
}

// Read the state from a file.
pub async fn read(path: &Path) -> io::Result<Durable> {
    // Read the file into a buffer.
    let mut file = File::open(path).await?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;

    // Deserialize the data.
    bincode::deserialize(&contents).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Error loading state file `{}`. Reason: {}",
                path.to_string_lossy(),
                error,
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use {
        crate::state::ProposalNumber,
        std::net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    #[test]
    fn proposal_ord_round() {
        let pn0 = ProposalNumber {
            round: 0,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8081),
        };

        let pn1 = ProposalNumber {
            round: 1,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        };

        assert!(pn1 > pn0);
    }

    #[test]
    fn proposal_ord_proposer_ip() {
        let pn0 = ProposalNumber {
            round: 0,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081),
        };

        let pn1 = ProposalNumber {
            round: 0,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8080),
        };

        assert!(pn1 > pn0);
    }

    #[test]
    fn proposal_ord_proposer_port() {
        let pn0 = ProposalNumber {
            round: 0,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        };

        let pn1 = ProposalNumber {
            round: 0,
            proposer_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081),
        };

        assert!(pn1 > pn0);
    }
}
