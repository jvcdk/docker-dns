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
}

impl CustomHandler {
    pub fn new(resolver: Arc<dyn DnsResolver>) -> Self {
        Self { resolver }
    }

    fn normalize_domain(name: &str) -> String {
        name.trim_end_matches('.').to_string()
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

        if let Some(dns_response) = self.resolver.resolve(&domain) {
            header.set_response_code(ResponseCode::NoError);
            header.set_authoritative(true);

            // Add all IPv4 addresses
            for ipv4 in dns_response.ipv4_addresses {
                let record = Record::from_rdata(query_name.clone().into(), 60, RData::A(ipv4.into()));
                result.push(record);
            }

            // Add all IPv6 addresses
            for ipv6 in dns_response.ipv6_addresses {
                let record = Record::from_rdata(query_name.clone().into(), 60, RData::AAAA(ipv6.into()));
                result.push(record);
            }
        } else {
            header.set_response_code(ResponseCode::NXDomain);
        };
        let response = builder.build(header, result.iter(), &[], &[], &[]);
        response_handle.send_response(response).await.unwrap()
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
                return ResponseInfo::from(request.request_info().header.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_domain_by_removing_trailing_dot() {
        assert_eq!(CustomHandler::normalize_domain("example.com."), "example.com");
        assert_eq!(CustomHandler::normalize_domain("example.com"), "example.com");
        assert_eq!(CustomHandler::normalize_domain("my.example.local."), "my.example.local");
    }
}
