use std::sync::atomic::{AtomicU64, Ordering};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use obscura_js::ops::{InterceptResolution, InterceptedRequest};
use obscura_net::interceptor::{InterceptAction, RequestInterceptor};
use obscura_net::{RequestInfo, ResourceType, Response};
use tokio::sync::mpsc::UnboundedSender;

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct PageRequestInterceptor {
    pub page_id: String,
    pub patterns: Vec<String>,
    pub tx: UnboundedSender<InterceptedRequest>,
}

#[async_trait::async_trait]
impl RequestInterceptor for PageRequestInterceptor {
    async fn intercept(&self, request: &RequestInfo) -> InterceptAction {
        if !self.patterns.iter().any(|pattern| url_matches_cdp_pattern(pattern, request.url.as_str())) {
            return InterceptAction::Continue;
        }

        let request_id = format!(
            "{}-request-{}",
            self.page_id,
            NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
        );
        let (resolver, resolved) = tokio::sync::oneshot::channel();
        let resource_type = match request.resource_type {
            ResourceType::Document => "Document",
            ResourceType::Script => "Script",
            ResourceType::Stylesheet => "Stylesheet",
            ResourceType::Image => "Image",
            ResourceType::Font => "Font",
            ResourceType::Xhr => "XHR",
            ResourceType::Fetch => "Fetch",
            ResourceType::Other => "Other",
        };
        if self.tx.send(InterceptedRequest {
            page_id: Some(self.page_id.clone()),
            request_id,
            url: request.url.to_string(),
            method: request.method.clone(),
            headers: request.headers.clone(),
            resource_type: resource_type.into(),
            resolver,
        }).is_err() {
            return InterceptAction::Block;
        }

        match resolved.await {
            Ok(InterceptResolution::Continue { url, method, headers, body }) => {
                if url.is_none() && method.is_none() && headers.is_none() && body.is_none() {
                    return InterceptAction::Continue;
                }
                let url = match url.map(|value| url::Url::parse(&value)).transpose() {
                    Ok(value) => value,
                    Err(_) => return InterceptAction::Block,
                };
                InterceptAction::ModifyRequest {
                    url,
                    method,
                    headers: headers.unwrap_or_default(),
                    body: body.map(String::into_bytes),
                }
            }
            Ok(InterceptResolution::Fulfill { status, headers, body, body_base64 }) => {
                let bytes = BASE64.decode(body_base64).unwrap_or_else(|_| body.into_bytes());
                InterceptAction::Fulfill(Response {
                    url: request.url.clone(),
                    status,
                    headers: headers.into_iter().map(|(key, value)| (key.to_ascii_lowercase(), value)).collect(),
                    body: bytes,
                    redirected_from: Vec::new(),
                })
            }
            Ok(InterceptResolution::Fail { .. }) | Err(_) => InterceptAction::Block,
        }
    }
}

pub(crate) fn url_matches_cdp_pattern(pattern: &str, url: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let mut remainder = url;
    let mut first = true;
    for part in pattern.split('*') {
        if part.is_empty() {
            continue;
        }
        let Some(index) = remainder.find(part) else {
            return false;
        };
        if first && !pattern.starts_with('*') && index != 0 {
            return false;
        }
        remainder = &remainder[index + part.len()..];
        first = false;
    }

    pattern.ends_with('*') || remainder.is_empty()
}
