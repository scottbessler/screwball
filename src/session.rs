use axum::{
    extract::Request,
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

/// The visitor's identity, derived from the `sid` cookie. Until passkey auth
/// lands this is simply a stable per-browser id.
#[derive(Clone, Copy, Debug)]
pub struct CurrentUser(pub Uuid);

const COOKIE_NAME: &str = "sid";
const ONE_YEAR_SECONDS: i64 = 60 * 60 * 24 * 365;

/// Middleware that ensures every request carries a `CurrentUser`, minting and
/// setting a new `sid` cookie when the visitor has none.
pub async fn attach_session(mut request: Request, next: Next) -> Response {
    let existing = read_sid(&request);
    let user = existing.unwrap_or_else(Uuid::new_v4);
    request.extensions_mut().insert(CurrentUser(user));

    let mut response = next.run(request).await;
    if existing.is_none() {
        let cookie = format!(
            "{COOKIE_NAME}={user}; Path=/; HttpOnly; SameSite=Lax; Max-Age={ONE_YEAR_SECONDS}"
        );
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

fn read_sid(request: &Request) -> Option<Uuid> {
    let cookies = request.headers().get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        if name == COOKIE_NAME {
            Uuid::parse_str(value).ok()
        } else {
            None
        }
    })
}
