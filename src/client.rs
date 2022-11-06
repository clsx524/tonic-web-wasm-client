use std::{future::Future, pin::Pin};
use ureq::OrAnyStatus;

use bytes::{Bytes, BytesMut};
use http::{
    header::{ACCEPT, CONTENT_TYPE},
    Request, Response,
};
use http_body::{combinators::UnsyncBoxBody};
use tonic::body::BoxBody;
use tower::Service;

use crate::{error::ClientError, grpc_response::GrpcResponse, utils::set_panic_hook};

/// `grpc-web` based transport layer for `tonic` clients
#[derive(Debug, Clone)]
pub struct Client {
    base_url: String,
}

impl Client {
    /// Creates a new client
    pub fn new(base_url: String) -> Self {
        set_panic_hook();
        Self { base_url }
    }
}

impl Service<Request<BoxBody>> for Client {
    type Response = Response<UnsyncBoxBody<Bytes, ClientError>>;

    type Error = ClientError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, req: Request<BoxBody>) -> Self::Future {
        Box::pin(request(self.base_url.clone(), req))
    }
}

async fn request(
    mut url: String,
    req: Request<BoxBody>,
) -> Result<Response<UnsyncBoxBody<Bytes, ClientError>>, ClientError> {
    url.push_str(&req.uri().to_string());

    let mut builder = ureq::post(&url)
        .set(CONTENT_TYPE.as_str(), "application/grpc-web+proto")
        .set(ACCEPT.as_str(), "application/grpc-web+proto")
        .set("x-grpc-web", "1");

    for (header_name, header_value) in req.headers().iter() {
        if header_name != CONTENT_TYPE && header_name != ACCEPT {
            builder = builder.set(header_name.as_str(), header_value.to_str()?);
        }
    }

    let response = builder.call().or_any_status()?;

    let mut result = Response::builder();
    result = result.status(response.status());

    for header_name in response.headers_names().iter() {
        match response.header(header_name) {
            Some(header_value) => {
                result = result.header(header_name.as_str(), header_value);
            }
            None => {}
        }
    }

    let content_type = match response.header(CONTENT_TYPE.as_str()) {
        None => Err(ClientError::MissingContentTypeHeader),
        Some(content_type) => Ok(content_type),
    }?
    .to_owned();

    let bytes = BytesMut::from(response.into_string()?.as_str());
    let body = UnsyncBoxBody::new(GrpcResponse::new(bytes, &content_type)?);

    result.body(body).map_err(Into::into)
}
