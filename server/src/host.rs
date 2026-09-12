use axum::{
    extract::{Request, State},
    http::{StatusCode, header::HOST, uri::Authority},
    middleware::Next,
    response::Response,
};

use crate::AppState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RequestHost {
    App,
    Blog(String),
}

pub(crate) async fn require_known_host(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let authority = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Authority>().ok())
        .ok_or(StatusCode::NOT_FOUND)?;

    let host = classify(authority.host(), &state.root_domain).ok_or(StatusCode::NOT_FOUND)?;
    request.extensions_mut().insert(host);
    Ok(next.run(request).await)
}

pub(crate) fn classify(host: &str, root_domain: &str) -> Option<RequestHost> {
    if matches!(host, "127.0.0.1" | "localhost") || host == format!("app.{root_domain}") {
        return Some(RequestHost::App);
    }

    let slug = host.strip_suffix(&format!(".{root_domain}"))?;
    if slug.contains('.') || slug.is_empty() {
        return None;
    }

    Some(RequestHost::Blog(slug.to_owned()))
}

pub(crate) fn require_app(host: &RequestHost) -> Result<(), StatusCode> {
    if host == &RequestHost::App {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[cfg(test)]
mod tests {
    use super::{RequestHost, classify};

    #[test]
    fn classifies_app_and_single_label_blog_hosts() {
        assert_eq!(
            classify("app.nabu.test", "nabu.test"),
            Some(RequestHost::App)
        );
        assert_eq!(
            classify("alice.nabu.test", "nabu.test"),
            Some(RequestHost::Blog("alice".to_owned()))
        );
        assert_eq!(classify("nested.alice.nabu.test", "nabu.test"), None);
        assert_eq!(classify("other.test", "nabu.test"), None);
    }
}
