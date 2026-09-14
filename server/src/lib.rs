use axum::{
    Json, Router,
    http::StatusCode,
    middleware,
    routing::{get, post, put},
};
use serde::Serialize;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;

pub mod auth;
mod blogs;
pub mod config;
mod content;
mod error;
mod host;
pub mod mail;
mod posts;
mod public;

#[derive(Clone)]
pub(crate) struct AppState {
    pool: PgPool,
    root_domain: String,
    auth: auth::AuthConfig,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

pub fn app(pool: PgPool, root_domain: String, auth: auth::AuthConfig) -> Router {
    let state = AppState {
        pool,
        root_domain,
        auth,
    };
    let api = Router::new()
        .route("/auth/email/start", post(auth::start_email))
        .route("/auth/email/verify", post(auth::verify_email))
        .route("/auth/logout", post(auth::logout))
        .route("/session", get(auth::session))
        .route("/blog", post(blogs::create))
        .route("/posts", get(posts::list).post(posts::create))
        .route(
            "/posts/{post_id}",
            get(posts::get).put(posts::update).delete(posts::delete),
        )
        .route("/posts/{post_id}/publication", put(posts::set_publication));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/", get(public::blog))
        .route("/feed.xml", get(public::feed))
        .route("/sitemap.xml", get(public::sitemap))
        .route("/robots.txt", get(public::robots))
        .route("/{post_slug}", get(public::post))
        .nest("/api/v1", api)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            host::require_known_host,
        ))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn ready(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<Health>, (StatusCode, Json<Health>)> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .map(|_| Json(Health { status: "ready" }))
        .map_err(|error| {
            tracing::warn!(%error, "database readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "unavailable",
                }),
            )
        })
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::{app, auth::AuthConfig};

    #[tokio::test]
    async fn health_is_available_without_database_access() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://nabu:nabu@127.0.0.1:5432/nabu")
            .expect("test database URL should parse");

        let response = app(
            pool,
            "nabu.test".to_owned(),
            AuthConfig::new(b"test-secret-that-is-at-least-32-bytes".to_vec(), false).unwrap(),
        )
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("host", "app.nabu.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
