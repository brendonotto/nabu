use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub(crate) struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    current_revision: Option<i64>,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_revision: Option<i64>,
}

impl ApiError {
    pub(crate) const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            current_revision: None,
        }
    }

    pub(crate) const fn stale_revision(current_revision: i64) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "stale_revision",
            message: "This post changed elsewhere. Reload the latest version before saving.",
            current_revision: Some(current_revision),
        }
    }

    pub(crate) fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, "request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "The request could not be completed.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                    current_revision: self.current_revision,
                },
            }),
        )
            .into_response()
    }
}
