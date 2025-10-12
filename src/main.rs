use docker_dns::resolver::StaticResolver;
use docker_dns::server::DnsServer;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut resolver = StaticResolver::new();
    resolver.add_mapping("my.example.local", Ipv4Addr::new(10, 11, 12, 13));

    let addr: SocketAddr = "0.0.0.0:53".parse()?;
    let server = DnsServer::new(Arc::new(resolver), addr);

    println!("DNS server starting on {}", addr);
    server.run().await
}
