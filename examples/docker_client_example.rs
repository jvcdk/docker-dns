use docker_dns::docker_client::{DockerClient, NetworkInfo, NetworkInfoProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = DockerClient::new_with_defaults()?;
    display_network_info(&client).await?;

    Ok(())
}

async fn display_network_info(provider: &impl NetworkInfoProvider) -> anyhow::Result<()> {
    println!("Fetching network information for running containers...\n");
    let network_infos = provider.list_containers_network_info().await?;

    if network_infos.is_empty() {
        println!("No network information found.");
        return Ok(());
    }

    for network_info in network_infos {
        print_network_info(&network_info);
    }

    Ok(())
}

fn print_network_info(network_info: &NetworkInfo) {
    if !network_info.names.is_empty() {
        println!("Container(s): {}", network_info.names.join(", "));
    }

    if !network_info.ipv4_addresses.is_empty() {
        println!(
            "  IPv4: {}",
            network_info
                .ipv4_addresses
                .iter()
                .map(|ip| ip.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    if !network_info.ipv6_addresses.is_empty() {
        println!(
            "  IPv6: {}",
            network_info
                .ipv6_addresses
                .iter()
                .map(|ip| ip.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    println!();
}
