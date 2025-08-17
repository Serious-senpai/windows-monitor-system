use async_trait::async_trait;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response};
use log::error;

use crate::models::users::User;
use crate::routes::abc::Service;
use crate::{required_header, utils};

pub struct LoginService;

#[async_trait]
impl Service for LoginService {
    fn route(&self) -> &'static str {
        "/login"
    }

    async fn serve(&self, request: Request<Incoming>) -> Response<BoxBody<Bytes, hyper::Error>> {
        if request.method() == Method::POST {
            let username = required_header!(request, "Username");
            let password = required_header!(request, "Password");

            match User::query(username).await {
                Ok(Some(user)) => {
                    if utils::check_password(password, &user.hashed_password) {
                        utils::ok_str(format!("Logged in as {}", user.username))
                    } else {
                        utils::forbidden()
                    }
                }
                Ok(None) => utils::forbidden(),
                Err(e) => {
                    error!("Error querying user {username:?}: {e}");
                    utils::service_unavailable()
                }
            }
        } else {
            utils::method_not_allowed()
        }
    }
}
