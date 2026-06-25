use std::convert::Infallible;

use axum::Json;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, Key, SameSite, SignedCookieJar};
use serde_json::json;
use uuid::Uuid;

use crate::error::AppError;

/// Name of the signed cookie that records the logged-in user's id.
pub const SESSION_COOKIE: &str = "sid";

/// Build the signed session cookie recording the authenticated user.
pub fn session_cookie(user: Uuid) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, user.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .permanent()
        .build()
}

fn read_user(jar: &SignedCookieJar) -> Option<Uuid> {
    jar.get(SESSION_COOKIE)
        .and_then(|cookie| Uuid::parse_str(cookie.value()).ok())
}

/// The authenticated visitor. Rejects with 401 when no valid session is present.
#[derive(Clone, Copy, Debug)]
pub struct AuthUser(pub Uuid);

/// The visitor's identity if they are signed in, or `None` for a guest.
#[derive(Clone, Copy, Debug)]
pub struct MaybeUser(pub Option<Uuid>);

impl<S> FromRequestParts<S> for MaybeUser
where
    S: Send + Sync,
    Key: FromRef<S>,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jar = SignedCookieJar::<Key>::from_request_parts(parts, state)
            .await
            .expect("SignedCookieJar extraction is infallible");
        Ok(MaybeUser(read_user(&jar)))
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    Key: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jar = SignedCookieJar::<Key>::from_request_parts(parts, state)
            .await
            .expect("SignedCookieJar extraction is infallible");
        read_user(&jar)
            .map(AuthUser)
            .ok_or_else(|| AppError::unauthorized("sign in with a passkey to do that"))
    }
}

/// Like [`AuthUser`], but for JSON API endpoints: rejects with a JSON body
/// (matching the `{ "error": ... }` shape used by move responses) so `fetch`
/// clients can parse the failure instead of choking on an HTML error page.
#[derive(Clone, Copy, Debug)]
pub struct ApiAuthUser(pub Uuid);

impl<S> FromRequestParts<S> for ApiAuthUser
where
    S: Send + Sync,
    Key: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jar = SignedCookieJar::<Key>::from_request_parts(parts, state)
            .await
            .expect("SignedCookieJar extraction is infallible");
        read_user(&jar).map(ApiAuthUser).ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "sign in with a passkey to do that" })),
            )
                .into_response()
        })
    }
}
