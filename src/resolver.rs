use async_trait::async_trait;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use crate::docker_client::{NetworkInfoProvider};
use log::error;

#[derive(Debug, Clone, PartialEq)]
pub struct DnsResponse {
    pub ipv4_addresses: Vec<Ipv4Addr>,
    pub ipv6_addresses: Vec<Ipv6Addr>,
}

impl DnsResponse {
    pub fn new(ipv4_addresses: Vec<Ipv4Addr>, ipv6_addresses: Vec<Ipv6Addr>) -> Self {
        Self {
            ipv4_addresses,
            ipv6_addresses,
        }
    }
}

#[async_trait]
pub trait DnsResolver: Send + Sync {
    async fn resolve(&self, domain: &str) -> Option<Arc<DnsResponse>>;
}

pub struct StaticResolver {
    mappings: HashMap<String, Ipv4Addr>,
}

impl StaticResolver {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, domain: impl Into<String>, ip: Ipv4Addr) {
        self.mappings.insert(domain.into(), ip);
    }
}

impl Default for StaticResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DnsResolver for StaticResolver {
    async fn resolve(&self, domain: &str) -> Option<Arc<DnsResponse>> {
        self.mappings.get(domain).map(|&ip| {
            Arc::new(DnsResponse::new(vec![ip], vec![]))
        })
    }
}

#[derive(Debug, Clone)]
pub struct DockerResolverConfig {
    pub hit_timeout: Duration,
    pub miss_timeout: Duration,
    pub refresh_timeout: Duration,
}

impl Default for DockerResolverConfig {
    fn default() -> Self {
        Self {
            hit_timeout: Duration::from_secs(60),
            miss_timeout: Duration::from_secs(5),
            refresh_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone)]
struct CachedNetworkData {
    mappings: HashMap<String, Arc<DnsResponse>>,
    last_refresh: Option<Instant>,
}

impl CachedNetworkData {
    fn new() -> Self {
        Self {
            mappings: HashMap::new(),
            last_refresh: None,
        }
    }

    fn is_older_than(&self, duration: Duration) -> bool {
        match self.last_refresh {
            None => true, // Never refreshed, so consider it old
            Some(instant) => instant.elapsed() > duration,
        }
    }
}

pub struct DockerResolver {
    provider: Arc<dyn NetworkInfoProvider>,
    config: DockerResolverConfig,
    cache: Arc<RwLock<CachedNetworkData>>,
}

impl DockerResolver {
    pub fn new(provider: impl NetworkInfoProvider + 'static, config: DockerResolverConfig) -> Self {
        Self {
            provider: Arc::new(provider),
            config,
            cache: Arc::new(RwLock::new(CachedNetworkData::new())),
        }
    }

    pub fn new_with_defaults(provider: impl NetworkInfoProvider + 'static) -> Self {
        Self::new(provider, DockerResolverConfig::default())
    }

    async fn refresh_cache(&self) -> anyhow::Result<()> {
        let mut cache = self.cache.write().await;

        if let Some(last_refresh) = cache.last_refresh && last_refresh.elapsed() < self.config.miss_timeout {
            return Ok(());
        }

        let mappings = tokio::time::timeout(
            self.config.refresh_timeout,
            self.fetch_and_build_mappings()
        )
        .await
        .map_err(|_| anyhow::anyhow!("Docker API refresh timeout after {:?}", self.config.refresh_timeout))??;

        cache.mappings = mappings;
        cache.last_refresh = Some(Instant::now());

        Ok(())
    }

    async fn fetch_and_build_mappings(&self) -> anyhow::Result<HashMap<String, Arc<DnsResponse>>>
    {
        let network_infos = self.provider.list_containers_network_info().await?;

        let mut mappings = HashMap::new();
        for info in network_infos {
            let response = Arc::new(DnsResponse::new(info.ipv4_addresses, info.ipv6_addresses));
            for name in info.names {
                mappings.insert(name, Arc::clone(&response));
            }
        }

        Ok(mappings)
    }
    
    async fn resolve_async(&self, domain: &str) -> Option<Arc<DnsResponse>> {
        // Read the cache first
        let (cached_result, hit_timeout_exceeded, miss_timeout_exceeded) = self.read_cache(domain).await;

        match (cached_result, hit_timeout_exceeded, miss_timeout_exceeded) {
            // Cache hit with fresh data
            (Some(response), false, _) => Some(response),

            // Cache is older than hit timeout - refresh regardless of hit/miss
            (_, true, _) => {
                self.get_refreshed_cache_entry(domain, "Failed to refresh DNS cache").await
            }

            // Cache miss, but within miss timeout - return None without refresh
            (None, false, false) => None,

            // Cache miss, and older than miss timeout - refresh and retry
            (None, _, true) => {
                self.get_refreshed_cache_entry(domain, "Failed to refresh DNS cache on miss").await
            }
        }
    }

    async fn get_refreshed_cache_entry(&self, domain: &str, err_context: &str) -> Option<Arc<DnsResponse>> {
        if let Err(e) = self.refresh_cache().await {
            error!("{}: {:#}", err_context, e);
        }

        let cache = self.cache.read().await;
        cache.mappings.get(domain).map(Arc::clone)
    }
    
    async fn read_cache(&self, domain: &str) -> (Option<Arc<DnsResponse>>, bool, bool) {
        let cache = self.cache.read().await;
        let result = cache.mappings.get(domain).map(Arc::clone);
        let hit_timeout_exceeded = cache.is_older_than(self.config.hit_timeout);
        let miss_timeout_exceeded = cache.is_older_than(self.config.miss_timeout);
        (result, hit_timeout_exceeded, miss_timeout_exceeded)
    }
}

#[async_trait]
impl DnsResolver for DockerResolver {
    async fn resolve(&self, domain: &str) -> Option<Arc<DnsResponse>> {
        self.resolve_async(domain).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker_client::NetworkInfo;
    use async_trait::async_trait;

    #[tokio::test]
    async fn resolves_configured_domain() {
        let mut resolver = StaticResolver::new();
        resolver.add_mapping("my.example.local", Ipv4Addr::new(10, 11, 12, 13));

        let result = resolver.resolve("my.example.local").await;

        assert!(result.is_some());
        let response = result.unwrap();
        assert_eq!(response.ipv4_addresses, vec![Ipv4Addr::new(10, 11, 12, 13)]);
        assert_eq!(response.ipv6_addresses, Vec::<Ipv6Addr>::new());
    }

    #[tokio::test]
    async fn returns_none_for_unknown_domain() {
        let resolver = StaticResolver::new();

        let result = resolver.resolve("unknown.domain").await;

        assert_eq!(result, None);
    }

    // Tests for DockerResolver
    struct MockNetworkInfoProvider {
        data: Vec<NetworkInfo>,
        call_count: Arc<RwLock<usize>>,
    }

    impl MockNetworkInfoProvider {
        fn new(data: Vec<NetworkInfo>) -> Self {
            Self {
                data,
                call_count: Arc::new(RwLock::new(0)),
            }
        }
    }

    #[async_trait]
    impl NetworkInfoProvider for MockNetworkInfoProvider {
        async fn list_containers_network_info(
            &self,
        ) -> Result<Vec<NetworkInfo>, anyhow::Error> {
            let mut count = self.call_count.write().await;
            *count += 1;
            Ok(self.data.clone())
        }
    }

    #[tokio::test]
    async fn docker_resolver_resolves_container_name() {
        let provider = MockNetworkInfoProvider::new(vec![NetworkInfo {
            names: vec!["container1".to_string()],
            ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2)],
            ipv6_addresses: vec![],
        }]);

        let resolver = DockerResolver::new_with_defaults(provider);

        let result = resolver.resolve("container1").await;

        assert!(result.is_some());
        let response = result.unwrap();
        assert_eq!(response.ipv4_addresses, vec![Ipv4Addr::new(172, 17, 0, 2)]);
        assert_eq!(response.ipv6_addresses, Vec::<Ipv6Addr>::new());
    }

    #[tokio::test]
    async fn docker_resolver_resolves_multiple_ips() {
        let ipv6 = "2001:db8::1".parse::<Ipv6Addr>().unwrap();
        let provider = MockNetworkInfoProvider::new(vec![NetworkInfo {
            names: vec!["multi-ip-container".to_string()],
            ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2), Ipv4Addr::new(172, 17, 0, 3)],
            ipv6_addresses: vec![ipv6],
        }]);

        let resolver = DockerResolver::new_with_defaults(provider);

        let result = resolver.resolve("multi-ip-container").await;

        assert!(result.is_some());
        let response = result.unwrap();
        assert_eq!(
            response.ipv4_addresses,
            vec![Ipv4Addr::new(172, 17, 0, 2), Ipv4Addr::new(172, 17, 0, 3)]
        );
        assert_eq!(response.ipv6_addresses, vec![ipv6]);
    }

    #[tokio::test]
    async fn docker_resolver_caches_on_hit() {
        let provider = MockNetworkInfoProvider::new(vec![NetworkInfo {
            names: vec!["container1".to_string()],
            ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2)],
            ipv6_addresses: vec![],
        }]);

        let call_count_tracker = provider.call_count.clone();

        let resolver = DockerResolver::new(provider, DockerResolverConfig::default());

        // First call should fetch from provider
        let _ = resolver.resolve("container1").await;
        assert_eq!(*call_count_tracker.read().await, 1);

        // Second call should use cache
        let _ = resolver.resolve("container1").await;
        assert_eq!(*call_count_tracker.read().await, 1);
    }

    #[tokio::test]
    async fn docker_resolver_refreshes_on_hit_timeout() {
        let provider = MockNetworkInfoProvider::new(vec![NetworkInfo {
            names: vec!["container1".to_string()],
            ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2)],
            ipv6_addresses: vec![],
        }]);

        let call_count_tracker = provider.call_count.clone();

        let config = DockerResolverConfig {
            hit_timeout: Duration::from_millis(50),
            miss_timeout: Duration::from_millis(10),
            refresh_timeout: Duration::from_secs(5),
        };

        let resolver = DockerResolver::new(provider, config);

        // First call
        let _ = resolver.resolve("container1").await;
        assert_eq!(*call_count_tracker.read().await, 1);

        // Wait for hit timeout to expire
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Second call should refresh
        let _ = resolver.resolve("container1").await;
        assert_eq!(*call_count_tracker.read().await, 2);
    }

    #[tokio::test]
    async fn docker_resolver_returns_none_on_miss_within_miss_timeout() {
        let provider = MockNetworkInfoProvider::new(vec![NetworkInfo {
            names: vec!["container1".to_string()],
            ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2)],
            ipv6_addresses: vec![],
        }]);

        let call_count_tracker = provider.call_count.clone();

        let resolver = DockerResolver::new(provider, DockerResolverConfig::default());

        // First call to populate cache
        let _ = resolver.resolve("container1").await;
        assert_eq!(*call_count_tracker.read().await, 1);

        // Query unknown domain - should return None without refresh
        let result = resolver.resolve("unknown").await;
        assert_eq!(result, None);
        assert_eq!(*call_count_tracker.read().await, 1);
    }

    #[tokio::test]
    async fn docker_resolver_refreshes_on_miss_after_miss_timeout() {
        let provider = MockNetworkInfoProvider::new(vec![NetworkInfo {
            names: vec!["container1".to_string()],
            ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2)],
            ipv6_addresses: vec![],
        }]);

        let call_count_tracker = provider.call_count.clone();

        let config = DockerResolverConfig {
            hit_timeout: Duration::from_secs(60),
            miss_timeout: Duration::from_millis(50),
            refresh_timeout: Duration::from_secs(5),
        };

        let resolver = DockerResolver::new(provider, config);

        // First call to populate cache
        let _ = resolver.resolve("container1").await;
        assert_eq!(*call_count_tracker.read().await, 1);

        // Wait for miss timeout to expire
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Query unknown domain - should refresh
        let result = resolver.resolve("unknown").await;
        assert_eq!(result, None);
        assert_eq!(*call_count_tracker.read().await, 2);
    }

    #[tokio::test]
    async fn docker_resolver_handles_multiple_names_per_container() {
        let provider = MockNetworkInfoProvider::new(vec![NetworkInfo {
            names: vec![
                "container1".to_string(),
                "container1.network1".to_string(),
                "alias1".to_string(),
            ],
            ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2)],
            ipv6_addresses: vec![],
        }]);

        let resolver = DockerResolver::new_with_defaults(provider);

        let result1 = resolver.resolve("container1").await;
        let result2 = resolver.resolve("container1.network1").await;
        let result3 = resolver.resolve("alias1").await;

        // All should resolve to the same values
        assert!(result1.is_some());
        assert!(result2.is_some());
        assert!(result3.is_some());

        let response1 = result1.unwrap();
        let response2 = result2.unwrap();
        let response3 = result3.unwrap();

        assert_eq!(response1.ipv4_addresses, vec![Ipv4Addr::new(172, 17, 0, 2)]);
        assert_eq!(response2.ipv4_addresses, vec![Ipv4Addr::new(172, 17, 0, 2)]);
        assert_eq!(response3.ipv4_addresses, vec![Ipv4Addr::new(172, 17, 0, 2)]);
    }

    // Mock provider that simulates a slow Docker API
    struct SlowNetworkInfoProvider {
        delay: Duration,
    }

    impl SlowNetworkInfoProvider {
        fn new(delay: Duration) -> Self {
            Self { delay }
        }
    }

    #[async_trait]
    impl NetworkInfoProvider for SlowNetworkInfoProvider {
        async fn list_containers_network_info(&self) -> Result<Vec<NetworkInfo>, anyhow::Error> {
            tokio::time::sleep(self.delay).await;
            Ok(vec![NetworkInfo {
                names: vec!["slow-container".to_string()],
                ipv4_addresses: vec![Ipv4Addr::new(172, 17, 0, 2)],
                ipv6_addresses: vec![],
            }])
        }
    }

    #[tokio::test]
    async fn docker_resolver_times_out_on_slow_refresh() {
        let provider = SlowNetworkInfoProvider::new(Duration::from_millis(200));

        let config = DockerResolverConfig {
            hit_timeout: Duration::from_secs(60),
            miss_timeout: Duration::from_secs(5),
            refresh_timeout: Duration::from_millis(50), // Short timeout
        };

        let resolver = DockerResolver::new(provider, config);

        // First call should timeout
        let result = resolver.resolve("slow-container").await;

        // Should return None because the refresh timed out
        assert_eq!(result, None);
    }
}
