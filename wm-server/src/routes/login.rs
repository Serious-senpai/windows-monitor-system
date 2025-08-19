use std::sync::Arc;

use async_trait::async_trait;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use log::error;
use openssl::base64::decode_block;

use crate::app::App;
use crate::models::users::User;
use crate::responses::ResponseBuilder;
use crate::routes::abc::Service;
use crate::{required_header, utils};

pub struct LoginService;

#[async_trait]
impl Service for LoginService {
    fn route(&self) -> &'static str {
        "/login"
    }

    async fn serve(
        &self,
        app: Arc<App>,
        request: Request<Incoming>,
    ) -> Response<BoxBody<Bytes, hyper::Error>> {
        if request.method() == Method::POST {
            let authorization = match decode_block(
                required_header!(request, "Authorization").trim_start_matches("Basic "),
            ) {
                Ok(data) => match String::from_utf8(data) {
                    Ok(data) => data,
                    Err(_) => return ResponseBuilder::default(StatusCode::BAD_REQUEST),
                },
                Err(_) => return ResponseBuilder::default(StatusCode::BAD_REQUEST),
            };

            let (username, password) = match authorization.split_once(':') {
                Some(p) => p,
                None => return ResponseBuilder::default(StatusCode::BAD_REQUEST),
            };

            match app.elastic().await {
                Some(elastic) => match User::query(elastic, username).await {
                    Ok(Some(user)) => {
                        if utils::check_password(password, &user.hashed_password) {
                            ResponseBuilder::default(StatusCode::OK)
                        } else {
                            ResponseBuilder::default(StatusCode::FORBIDDEN)
                        }
                    }
                    Ok(None) => ResponseBuilder::default(StatusCode::FORBIDDEN),
                    Err(e) => {
                        error!("Error querying user {username:?}: {e}");
                        ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE)
                    }
                },
                None => ResponseBuilder::default(StatusCode::SERVICE_UNAVAILABLE),
            }
        } else {
            ResponseBuilder::default(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}
