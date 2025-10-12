use docker_dns::resolver::StaticResolver;
use docker_dns::server::DnsServer;
use hickory_client::client::{AsyncClient, ClientHandle};
use hickory_client::rr::{DNSClass, Name, RecordType};
use hickory_client::udp::UdpClientStream;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_resolves_my_example_local_to_static_ip() {
    let server_addr: SocketAddr = "127.0.0.1:5353".parse().unwrap();

    let mut resolver = StaticResolver::new();
    resolver.add_mapping("my.example.local", Ipv4Addr::new(10, 11, 12, 13));

    let server = DnsServer::new(Arc::new(resolver), server_addr);

    tokio::spawn(async move {
        server.run().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stream = UdpClientStream::<tokio::net::UdpSocket>::new(server_addr);
    let (mut client, bg) = AsyncClient::connect(stream).await.unwrap();

    tokio::spawn(bg);

    let name = Name::from_str("my.example.local").unwrap();
    let response = client.query(name, DNSClass::IN, RecordType::A).await.unwrap();

    let answers = response.answers();
    assert_eq!(answers.len(), 1, "Expected exactly one answer");

    let record = &answers[0];
    let ip = record.data().unwrap().as_a().unwrap();

    assert_eq!(ip.0, Ipv4Addr::new(10, 11, 12, 13));
}
