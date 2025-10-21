use crate::resolver::DnsResolver;
use async_trait::async_trait;
use hickory_server::authority::MessageResponseBuilder;
use hickory_server::proto::op::{Header, MessageType, ResponseCode};
use hickory_server::proto::rr::{RData, Record};
use hickory_server::server::{Request, RequestHandler, ResponseHandler, ResponseInfo};
use log::error;
use std::sync::Arc;

pub struct CustomHandler {
    resolver: Arc<dyn DnsResolver>,
    suffix: String,
    ttl: u32,
}

impl CustomHandler {
    pub fn new(resolver: Arc<dyn DnsResolver>, suffix: String, ttl: u32) -> Self {
        Self { resolver, suffix, ttl }
    }

    fn normalize_domain(name: &str) -> String {
        name.trim_end_matches('.').to_string()
    }

    /// Checks if the domain matches the configured suffix and strips it
    /// Returns Some(stripped_name) if it matches, None if it doesn't
    fn strip_suffix(&self, domain: &str) -> Option<String> {
        if self.suffix.is_empty() {
            return Some(domain.to_string()); // No suffix filter, accept all
        }

        if domain.ends_with(&self.suffix) {
            let stripped = &domain[..domain.len() - self.suffix.len()];
            Some(stripped.to_string())
        } else {
            None // Domain doesn't match suffix, reject
        }
    }

    async fn handle_query<R: ResponseHandler>(
        &self,
        request: &Request,
        mut response_handle: R,
    ) -> ResponseInfo {
        let request_info = request.request_info();
        let query_name = request_info.query.name();
        let domain = Self::normalize_domain(&query_name.to_string());

        let builder = MessageResponseBuilder::from_message_request(request);
        let mut header = Header::response_from_request(request_info.header);

        let mut result: Vec<Record> = vec![];

        // Check if domain matches suffix filter and strip it
        match self.strip_suffix(&domain) {
            Some(container_name) => {
                // Domain matches suffix (or no suffix configured), look it up
                if let Some(dns_response) = self.resolver.resolve(&container_name).await {
                    header.set_response_code(ResponseCode::NoError);
                    header.set_authoritative(true);

                    // Add all IPv4 addresses
                    for ipv4 in &dns_response.ipv4_addresses {
                        let record = Record::from_rdata(query_name.clone().into(), self.ttl, RData::A((*ipv4).into()));
                        result.push(record);
                    }

                    // Add all IPv6 addresses
                    for ipv6 in &dns_response.ipv6_addresses {
                        let record = Record::from_rdata(query_name.clone().into(), self.ttl, RData::AAAA((*ipv6).into()));
                        result.push(record);
                    }
                } else {
                    // Container not found
                    header.set_response_code(ResponseCode::NXDomain);
                }
            }
            None => {
                // Domain doesn't match suffix filter, refuse to answer
                header.set_response_code(ResponseCode::Refused);
            }
        }

        let response = builder.build(header, result.iter(), &[], &[], &[]);
        match response_handle.send_response(response).await {
            Ok(info) => info,
            Err(e) => {
                error!("Failed to send DNS response: {:#}", e);
                ResponseInfo::from(*request_info.header)
            }
        }
    }
}

#[async_trait]
impl RequestHandler for CustomHandler {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        response_handle: R,
    ) -> ResponseInfo {
        let header = request.request_info().header;
        match header.message_type() {
            MessageType::Query => self.handle_query(request, response_handle).await,
            MessageType::Response => {
                error!("Unexpected message type: Response. Dropping request.");
                // Return early - no need to send a response to a response
                return ResponseInfo::from(*request.request_info().header);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::StaticResolver;

    #[test]
    fn normalizes_domain_by_removing_trailing_dot() {
        assert_eq!(CustomHandler::normalize_domain("example.com."), "example.com");
        assert_eq!(CustomHandler::normalize_domain("example.com"), "example.com");
        assert_eq!(CustomHandler::normalize_domain("my.example.local."), "my.example.local");
    }

    #[test]
    fn strips_suffix_when_configured() {
        let resolver = Arc::new(StaticResolver::new());
        let handler = CustomHandler::new(resolver, ".docker".to_string(), 60);

        assert_eq!(handler.strip_suffix("myapp.docker"), Some("myapp".to_string()));
        assert_eq!(handler.strip_suffix("nginx.docker"), Some("nginx".to_string()));
        assert_eq!(handler.strip_suffix("example.com"), None);
    }

    #[test]
    fn accepts_all_domains_when_no_suffix_configured() {
        let resolver = Arc::new(StaticResolver::new());
        let handler = CustomHandler::new(resolver, "".to_string(), 60);

        assert_eq!(handler.strip_suffix("myapp.docker"), Some("myapp.docker".to_string()));
        assert_eq!(handler.strip_suffix("example.com"), Some("example.com".to_string()));
        assert_eq!(handler.strip_suffix("anything"), Some("anything".to_string()));
    }

    #[test]
    fn handles_nested_domain_with_suffix() {
        let resolver = Arc::new(StaticResolver::new());
        let handler = CustomHandler::new(resolver, ".docker".to_string(), 60);

        assert_eq!(
            handler.strip_suffix("app.production.docker"),
            Some("app.production".to_string())
        );
    }
}
