use crate::custom_handler::CustomHandler;
use crate::resolver::DnsResolver;
use anyhow::Result;
use hickory_server::ServerFuture;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

pub struct DnsServer {
    resolver: Arc<dyn DnsResolver>,
    bind_addr: SocketAddr,
    suffix: String,
}

impl DnsServer {
    pub fn new(resolver: Arc<dyn DnsResolver>, bind_addr: SocketAddr, suffix: String) -> Self {
        Self {
            resolver,
            bind_addr,
            suffix,
        }
    }

    pub async fn run(self) -> Result<()> {
        let handler = CustomHandler::new(self.resolver, self.suffix);
        let mut server = ServerFuture::new(handler);

        let socket = UdpSocket::bind(self.bind_addr).await?;
        server.register_socket(socket);

        server.block_until_done().await?;
        Ok(())
    }
}
