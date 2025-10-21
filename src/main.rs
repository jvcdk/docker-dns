use clap::Parser;
use docker_dns::docker_client::{DockerClient, DockerClientConfig};
use docker_dns::resolver::{DockerResolver, DockerResolverConfig};
use docker_dns::server::DnsServer;
use env_logger::Builder;
use log::LevelFilter;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;

/// DNS server that resolves Docker container names to their IP addresses
#[derive(Parser, Debug)]
#[command(name = "docker-dns")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// DNS server bind address
    #[arg(short, long, default_value = "0.0.0.0:53")]
    bind: String,

    /// Docker socket path
    #[arg(short, long, default_value = "/var/run/docker.sock")]
    socket: String,

    /// Cache hit timeout in seconds (how long to cache successful lookups)
    #[arg(long, default_value = "60")]
    hit_timeout: u64,

    /// Cache miss timeout in seconds (how long to wait before retrying failed lookups)
    #[arg(long, default_value = "5")]
    miss_timeout: u64,

    /// Docker API communication timeout in seconds
    #[arg(long, default_value = "5")]
    docker_timeout: u64,

    /// DNS suffix to filter queries (e.g., "docker" or ".docker")
    /// Only queries ending with this suffix will be resolved
    /// The suffix will be stripped before looking up container names
    #[arg(long, default_value = "")]
    suffix: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logger
    Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .init();

    // Normalize suffix to ensure it starts with a dot if not empty
    let suffix = if args.suffix.is_empty() {
        String::new()
    } else if args.suffix.starts_with('.') {
        args.suffix
    } else {
        format!(".{}", args.suffix)
    };

    // Print configuration to stdout (always visible)
    println!("Docker DNS Server v{}", env!("CARGO_PKG_VERSION"));
    println!("Configuration:");
    println!("  Bind address: {}", args.bind);
    println!("  Docker socket: {}", args.socket);
    println!("  Hit timeout: {}s", args.hit_timeout);
    println!("  Miss timeout: {}s", args.miss_timeout);
    println!("  Docker timeout: {}s", args.docker_timeout);
    if suffix.is_empty() {
        println!("  DNS suffix: (none - resolving all queries)");
    } else {
        println!("  DNS suffix: {}", suffix);
    }
    println!();


    // Create Docker client
    let docker_config = DockerClientConfig {
        socket_path: args.socket,
        timeout_seconds: args.docker_timeout,
    };
    let docker_client = DockerClient::new(docker_config)?;
    println!("✓ Connected to Docker daemon");

    // Create DNS resolver with caching
    let resolver_config = DockerResolverConfig {
        hit_timeout: Duration::from_secs(args.hit_timeout),
        miss_timeout: Duration::from_secs(args.miss_timeout),
        refresh_timeout: Duration::from_secs(args.docker_timeout),
    };
    let resolver = DockerResolver::new(docker_client, resolver_config);
    println!("✓ DNS resolver initialized");

    // Parse bind address and start DNS server
    let addr: SocketAddr = args.bind.parse()?;
    let ttl = args.hit_timeout as u32;
    let server = DnsServer::new(Arc::new(resolver), addr, suffix, ttl);

    println!("✓ DNS server starting on {}", addr);
    println!("\nServer is running. Press Ctrl+C to stop\n");

    tokio::select! {
        result = server.run() => result,
        _ = signal::ctrl_c() => {
            println!("\nShutdown signal received, stopping server...");
            Ok(())
        }
    }
}
