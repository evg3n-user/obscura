use std::collections::HashMap;

use url::Url;

use crate::client::{RequestInfo, Response};

pub enum InterceptAction {
    Continue,
    Block,
    Fulfill(Response),
    ModifyHeaders(HashMap<String, String>),
    ModifyRequest {
        url: Option<Url>,
        method: Option<String>,
        headers: HashMap<String, String>,
        body: Option<Vec<u8>>,
    },
}

#[async_trait::async_trait]
pub trait RequestInterceptor {
    async fn intercept(&self, request: &RequestInfo) -> InterceptAction;
}
