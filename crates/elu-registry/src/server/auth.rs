use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

/// Extracted publisher identity from bearer token.
#[derive(Debug, Clone)]
pub struct Publisher(pub String);

/// Optional publisher — present if Authorization header is provided.
#[derive(Debug, Clone)]
pub struct OptionalPublisher(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for Publisher {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Simple: treat the token as the publisher identity.
        // A real system would validate against an auth provider.
        if token.is_empty() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(Publisher(token.to_string()))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for OptionalPublisher {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let publisher = match parts.headers.get("authorization") {
            Some(header) => {
                let s = header.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
                let token = s
                    .strip_prefix("Bearer ")
                    .ok_or(StatusCode::UNAUTHORIZED)?;
                if token.is_empty() {
                    return Err(StatusCode::UNAUTHORIZED);
                }
                Some(token.to_string())
            }
            None => None,
        };
        Ok(OptionalPublisher(publisher))
    }
}
