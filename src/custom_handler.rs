use crate::resolver::DnsResolver;
use async_trait::async_trait;
use hickory_server::authority::MessageResponseBuilder;
use hickory_server::proto::op::{Header, MessageType, ResponseCode};
use hickory_server::proto::rr::{RData, Record, RecordType};
use hickory_server::server::{Request, RequestHandler, ResponseHandler, ResponseInfo};
use log::{error, warn};
use std::sync::Arc;

// Standard DNS UDP packet size limit (without EDNS)
const DNS_UDP_MAX_SIZE: usize = 512;

// Estimated overhead for DNS header and question section
// Header: 12 bytes, Question: ~name_length + 4 bytes
// We use a conservative estimate
const DNS_OVERHEAD_ESTIMATE: usize = 50;

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
        let query_type = request_info.query.query_type();
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

                    // Build records based on query type
                    let mut records = Vec::new();

                    match query_type {
                        RecordType::A => {
                            // Only return A records for A queries
                            for ipv4 in &dns_response.ipv4_addresses {
                                let record = Record::from_rdata(
                                    query_name.clone().into(),
                                    self.ttl,
                                    RData::A((*ipv4).into())
                                );
                                records.push(record);
                            }
                        }
                        RecordType::AAAA => {
                            // Only return AAAA records for AAAA queries
                            for ipv6 in &dns_response.ipv6_addresses {
                                let record = Record::from_rdata(
                                    query_name.clone().into(),
                                    self.ttl,
                                    RData::AAAA((*ipv6).into())
                                );
                                records.push(record);
                            }
                        }
                        _ => {
                            // For other query types, return empty response with NoError
                            // This is standard DNS behavior for unsupported query types
                        }
                    }

                    // Apply size limit to prevent exceeding UDP packet size
                    result = Self::apply_size_limit(records, &domain, query_name.len());
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

    /// Applies DNS UDP packet size limit, preferring to keep records that fit
    /// Returns as many records as will fit within the size limit
    fn apply_size_limit(records: Vec<Record>, domain: &str, query_name_len: usize) -> Vec<Record> {
        if records.is_empty() {
            return records;
        }

        let available_space = DNS_UDP_MAX_SIZE.saturating_sub(DNS_OVERHEAD_ESTIMATE + query_name_len);

        let mut result = Vec::new();
        let mut current_size = 0;
        let total_records = records.len();

        for record in records {
            // Estimate record size:
            // Name (compressed, usually 2 bytes pointer)
            // Type (2 bytes) + Class (2 bytes) + TTL (4 bytes) + RDLength (2 bytes)
            // RData: 4 bytes for A, 16 bytes for AAAA
            let record_size = match record.data() {
                Some(RData::A(_)) => 2 + 2 + 2 + 4 + 2 + 4,  // ~16 bytes
                Some(RData::AAAA(_)) => 2 + 2 + 2 + 4 + 2 + 16,  // ~28 bytes
                _ => 0, // We don't have any other records
            };

            if current_size + record_size > available_space {
                warn!(
                    "DNS response for '{}' truncated: {} records included, {} dropped (size limit)",
                    domain,
                    result.len(),
                    total_records - result.len()
                );
                break;
            }

            current_size += record_size;
            result.push(record);
        }

        result
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
