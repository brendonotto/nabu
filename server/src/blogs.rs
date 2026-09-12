use axum::{
    Extension, Json,
    http::{HeaderMap, StatusCode},
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use sqlx::error::DatabaseError;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{self, BlogSummary},
    error::ApiError,
    host,
};

const RESERVED_SLUGS: &[&str] = &[
    "api", "app", "assets", "auth", "blog", "help", "mail", "media", "status", "support", "www",
];

#[derive(Deserialize)]
pub(crate) struct CreateBlogRequest {
    title: String,
    slug: String,
}

pub(crate) async fn create(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(request): Json<CreateBlogRequest>,
) -> Result<(StatusCode, Json<BlogSummary>), ApiError> {
    host::require_app(&request_host).map_err(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "The resource was not found.",
        )
    })?;
    let session = auth::require_session_and_csrf(&state, &jar, &headers).await?;

    let title = request.title.trim();
    let slug = request.slug.trim().to_lowercase();
    if title.is_empty() || title.chars().count() > 120 {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_blog_title",
            "Blog titles must be between 1 and 120 characters.",
        ));
    }
    if !valid_slug(&slug) || RESERVED_SLUGS.contains(&slug.as_str()) {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_blog_slug",
            "Use 3–63 lowercase letters, numbers, or hyphens.",
        ));
    }

    let mut transaction = state.pool.begin().await.map_err(ApiError::internal)?;
    let already_owns_blog = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM blog_members WHERE account_id = $1 AND role = 'owner')",
    )
    .bind(session.account_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;
    if already_owns_blog {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "blog_already_exists",
            "This account already owns a blog.",
        ));
    }

    let blog = BlogSummary {
        id: Uuid::now_v7(),
        slug,
        title: title.to_owned(),
    };
    let insert = sqlx::query("INSERT INTO blogs (id, slug, title) VALUES ($1, $2, $3)")
        .bind(blog.id)
        .bind(&blog.slug)
        .bind(&blog.title)
        .execute(&mut *transaction)
        .await;
    if let Err(error) = insert {
        if error
            .as_database_error()
            .and_then(DatabaseError::code)
            .is_some_and(|code| code == "23505")
        {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "blog_slug_unavailable",
                "That blog address is already taken.",
            ));
        }
        return Err(ApiError::internal(error));
    }

    sqlx::query(
        "INSERT INTO blog_members (blog_id, account_id, role, accepted_at) \
         VALUES ($1, $2, 'owner', now())",
    )
    .bind(blog.id)
    .bind(session.account_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;

    Ok((StatusCode::CREATED, Json(blog)))
}

fn valid_slug(slug: &str) -> bool {
    (3..=63).contains(&slug.len())
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
}

#[cfg(test)]
mod tests {
    use super::valid_slug;

    #[test]
    fn slug_validation_covers_boundaries() {
        assert!(valid_slug("a-blog"));
        assert!(valid_slug("abc"));
        assert!(!valid_slug("ab"));
        assert!(!valid_slug("-blog"));
        assert!(!valid_slug("blog-"));
        assert!(!valid_slug("two--hyphens"));
        assert!(!valid_slug("Uppercase"));
    }
}
