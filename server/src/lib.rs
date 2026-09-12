use axum::{Json, Router, http::StatusCode, routing::get};
use serde::Serialize;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;

pub mod config;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

pub fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .with_state(AppState { pool })
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

    use super::app;

    #[tokio::test]
    async fn health_is_available_without_database_access() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://nabu:nabu@127.0.0.1:5432/nabu")
            .expect("test database URL should parse");

        let response = app(pool)
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
