use anyhow::{Context, Result};
use async_trait::async_trait;
use bollard::Docker;
use std::net::{Ipv4Addr, Ipv6Addr};
use strip_prefix_suffix_sane::StripPrefixSuffixSane;

#[derive(Debug, Clone)]
pub struct DockerClientConfig {
    pub socket_path: String,
    pub timeout_seconds: u64,
}

impl Default for DockerClientConfig {
    fn default() -> Self {
        Self {
            socket_path: "/var/run/docker.sock".to_string(),
            timeout_seconds: 120,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub names: Vec<String>,
    pub ipv4_addresses: Vec<Ipv4Addr>,
    pub ipv6_addresses: Vec<Ipv6Addr>,
}

#[async_trait]
pub trait NetworkInfoProvider: Send + Sync {
    async fn list_containers_network_info(&self) -> Result<Vec<NetworkInfo>>;
}

pub struct DockerClient {
    client: Docker,
}

impl DockerClient {
    /// Creates a new Docker client with the specified configuration
    ///
    /// # Arguments
    /// * `config` - Docker client configuration
    ///
    /// # Returns
    /// Result containing the DockerClient or an error
    pub fn new(config: DockerClientConfig) -> Result<Self> {
        let client = Docker::connect_with_socket(
            &config.socket_path,
            config.timeout_seconds,
            bollard::API_DEFAULT_VERSION,
        )
        .with_context(|| format!("Failed to connect to Docker socket at {}", config.socket_path))?;

        Ok(Self { client })
    }

    pub fn new_with_defaults() -> Result<Self> {
        Self::new(DockerClientConfig::default())
    }
}

#[async_trait]
impl NetworkInfoProvider for DockerClient {
    async fn list_containers_network_info(&self) -> Result<Vec<NetworkInfo>> {
        let containers = self
            .client
            .list_containers::<String>(None)
            .await
            .context("Failed to list containers")?;

        let mut result = Vec::new();

        for container in containers {
            let names = get_names(&container);

            let (ipv4_addresses, ipv6_addresses) = get_ip_addresses(container);

            if !ipv4_addresses.is_empty() || !ipv6_addresses.is_empty() {
                result.push(NetworkInfo {
                    names,
                    ipv4_addresses,
                    ipv6_addresses,
                });
            }
        }

        Ok(result)
    }
}

fn get_ip_addresses(container: bollard::secret::ContainerSummary) -> (Vec<Ipv4Addr>, Vec<Ipv6Addr>) {
    let mut ipv4_addresses = vec![];
    let mut ipv6_addresses = vec![];

    let Some(settings) = container.network_settings else {
        return (ipv4_addresses, ipv6_addresses);
    };

    let Some(networks_data) = settings.networks else {
        return (ipv4_addresses, ipv6_addresses);
    };

    for (_, endpoint) in networks_data {
        if let Some(ipv4) = endpoint.ip_address.as_deref().and_then(parse_ipv4) {
            ipv4_addresses.push(ipv4);
        }

        if let Some(ipv6) = endpoint.global_ipv6_address.as_deref().and_then(parse_ipv6) {
            ipv6_addresses.push(ipv6);
        }
    }

    (ipv4_addresses, ipv6_addresses)
}

fn parse_ipv4(ip_str: &str) -> Option<Ipv4Addr> {
    if ip_str.is_empty() {
        return None;
    }
    ip_str.parse::<Ipv4Addr>().ok()
}

fn parse_ipv6(ip_str: &str) -> Option<Ipv6Addr> {
    if ip_str.is_empty() {
        return None;
    }
    ip_str.parse::<Ipv6Addr>().ok()
}

fn get_names(container: &bollard::secret::ContainerSummary) -> Vec<String> {
    if let Some(names) = &container.names {
        names
            .iter()
            .map(|name| name.strip_prefix_sane("/").to_owned())
            .collect()
    } else {
        vec![]
    }
}
