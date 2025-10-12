use std::collections::HashMap;
use std::net::Ipv4Addr;

pub trait DnsResolver: Send + Sync {
    fn resolve(&self, domain: &str) -> Option<Ipv4Addr>;
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

impl DnsResolver for StaticResolver {
    fn resolve(&self, domain: &str) -> Option<Ipv4Addr> {
        self.mappings.get(domain).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_configured_domain() {
        let mut resolver = StaticResolver::new();
        resolver.add_mapping("my.example.local", Ipv4Addr::new(10, 11, 12, 13));

        let result = resolver.resolve("my.example.local");

        assert_eq!(result, Some(Ipv4Addr::new(10, 11, 12, 13)));
    }

    #[test]
    fn returns_none_for_unknown_domain() {
        let resolver = StaticResolver::new();

        let result = resolver.resolve("unknown.domain");

        assert_eq!(result, None);
    }
}
