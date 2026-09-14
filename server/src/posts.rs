use axum::{
    Extension, Json,
    extract::Path,
    http::{HeaderMap, StatusCode},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, error::DatabaseError};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{AppState, auth, content, error::ApiError, host};

const MAX_TITLE_CHARS: usize = 200;
const MAX_SUMMARY_CHARS: usize = 500;

#[derive(Serialize, sqlx::FromRow)]
pub(crate) struct PostSummary {
    id: Uuid,
    slug: String,
    title: String,
    summary: Option<String>,
    revision: i64,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(Serialize)]
pub(crate) struct PostResponse {
    id: Uuid,
    slug: String,
    title: String,
    summary: Option<String>,
    content_json: Value,
    revision: i64,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct PostRow {
    id: Uuid,
    slug: String,
    title: String,
    summary: Option<String>,
    content_json: Value,
    revision: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(Deserialize)]
pub(crate) struct CreatePostRequest {
    title: String,
    slug: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default = "empty_document")]
    content_json: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpdatePostRequest {
    revision: i64,
    title: String,
    slug: String,
    #[serde(default)]
    summary: Option<String>,
    content_json: Value,
}

#[derive(Deserialize)]
pub(crate) struct DeletePostRequest {
    revision: i64,
}

pub(crate) async fn list(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<PostSummary>>, ApiError> {
    require_app_host(&request_host)?;
    let session = auth::require_session(&state, &jar).await?;
    let posts = sqlx::query_as::<_, PostSummary>(
        "SELECT p.id, p.slug::text AS slug, p.title, p.summary, p.revision, p.updated_at \
         FROM posts p JOIN blogs b ON b.id = p.blog_id \
         JOIN blog_members m ON m.blog_id = p.blog_id \
         WHERE m.account_id = $1 AND m.accepted_at IS NOT NULL \
           AND b.deleted_at IS NULL AND p.deleted_at IS NULL \
         ORDER BY p.updated_at DESC, p.id DESC",
    )
    .bind(session.account_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(posts))
}

pub(crate) async fn create(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(request): Json<CreatePostRequest>,
) -> Result<(StatusCode, Json<PostResponse>), ApiError> {
    require_app_host(&request_host)?;
    let session = auth::require_session_and_csrf(&state, &jar, &headers).await?;
    let input = validate_input(
        &request.title,
        &request.slug,
        request.summary,
        request.content_json,
    )?;
    let blog_id = member_blog_id(&state.pool, session.account_id).await?;
    let row = sqlx::query_as::<_, PostRow>(
        "INSERT INTO posts \
         (id, blog_id, slug, title, summary, content_json, rendered_html, created_by, updated_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8) \
         RETURNING id, slug::text AS slug, title, summary, content_json, revision, created_at, updated_at",
    )
    .bind(Uuid::now_v7())
    .bind(blog_id)
    .bind(&input.slug)
    .bind(&input.title)
    .bind(&input.summary)
    .bind(&input.content_json)
    .bind(&input.rendered_html)
    .bind(session.account_id)
    .fetch_one(&state.pool)
    .await
    .map_err(map_write_error)?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

pub(crate) async fn get(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    Path(post_id): Path<Uuid>,
) -> Result<Json<PostResponse>, ApiError> {
    require_app_host(&request_host)?;
    let session = auth::require_session(&state, &jar).await?;
    let row = find_post(&state.pool, session.account_id, post_id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(row.into()))
}

pub(crate) async fn update(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(post_id): Path<Uuid>,
    Json(request): Json<UpdatePostRequest>,
) -> Result<Json<PostResponse>, ApiError> {
    require_app_host(&request_host)?;
    let session = auth::require_session_and_csrf(&state, &jar, &headers).await?;
    if request.revision < 1 {
        return Err(invalid_revision());
    }
    let input = validate_input(
        &request.title,
        &request.slug,
        request.summary,
        request.content_json,
    )?;
    let row = sqlx::query_as::<_, PostRow>(
        "UPDATE posts p SET slug = $1, title = $2, summary = $3, content_json = $4, \
             rendered_html = $5, revision = p.revision + 1, updated_by = $6, updated_at = now() \
         WHERE p.id = $7 AND p.revision = $8 AND p.deleted_at IS NULL \
           AND EXISTS (SELECT 1 FROM blog_members m JOIN blogs b ON b.id = m.blog_id \
                       WHERE m.blog_id = p.blog_id AND m.account_id = $6 \
                         AND m.accepted_at IS NOT NULL AND b.deleted_at IS NULL) \
         RETURNING p.id, p.slug::text AS slug, p.title, p.summary, p.content_json, \
                   p.revision, p.created_at, p.updated_at",
    )
    .bind(&input.slug)
    .bind(&input.title)
    .bind(&input.summary)
    .bind(&input.content_json)
    .bind(&input.rendered_html)
    .bind(session.account_id)
    .bind(post_id)
    .bind(request.revision)
    .fetch_optional(&state.pool)
    .await
    .map_err(map_write_error)?;

    match row {
        Some(row) => Ok(Json(row.into())),
        None => Err(current_revision_or_not_found(&state.pool, session.account_id, post_id).await?),
    }
}

pub(crate) async fn delete(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Path(post_id): Path<Uuid>,
    Json(request): Json<DeletePostRequest>,
) -> Result<StatusCode, ApiError> {
    require_app_host(&request_host)?;
    let session = auth::require_session_and_csrf(&state, &jar, &headers).await?;
    if request.revision < 1 {
        return Err(invalid_revision());
    }
    let result = sqlx::query(
        "UPDATE posts p SET deleted_at = now(), updated_by = $1, \
             revision = p.revision + 1, updated_at = now() \
         WHERE p.id = $2 AND p.revision = $3 AND p.deleted_at IS NULL \
           AND EXISTS (SELECT 1 FROM blog_members m JOIN blogs b ON b.id = m.blog_id \
                       WHERE m.blog_id = p.blog_id AND m.account_id = $1 \
                         AND m.accepted_at IS NOT NULL AND b.deleted_at IS NULL)",
    )
    .bind(session.account_id)
    .bind(post_id)
    .bind(request.revision)
    .execute(&state.pool)
    .await
    .map_err(ApiError::internal)?;
    if result.rows_affected() == 1 {
        return Ok(StatusCode::NO_CONTENT);
    }
    Err(current_revision_or_not_found(&state.pool, session.account_id, post_id).await?)
}

struct ValidatedInput {
    title: String,
    slug: String,
    summary: Option<String>,
    content_json: Value,
    rendered_html: String,
}

fn validate_input(
    title: &str,
    slug: &str,
    summary: Option<String>,
    content_json: Value,
) -> Result<ValidatedInput, ApiError> {
    let title = title.trim().to_owned();
    let slug = slug.trim().to_lowercase();
    let summary = summary
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_post_title",
            "Post titles must be between 1 and 200 characters.",
        ));
    }
    if !valid_slug(&slug) {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_post_slug",
            "Use 2–80 lowercase letters, numbers, or hyphens.",
        ));
    }
    if summary
        .as_ref()
        .is_some_and(|value| value.chars().count() > MAX_SUMMARY_CHARS)
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_post_summary",
            "Post summaries cannot exceed 500 characters.",
        ));
    }
    let content = content::validate_and_render(content_json).map_err(|_| invalid_content())?;
    Ok(ValidatedInput {
        title,
        slug,
        summary,
        content_json: content.document,
        rendered_html: content.rendered_html,
    })
}

fn valid_slug(slug: &str) -> bool {
    (2..=80).contains(&slug.len())
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

fn empty_document() -> Value {
    json!({ "type": "doc", "content": [{ "type": "paragraph" }] })
}

async fn member_blog_id(pool: &PgPool, account_id: Uuid) -> Result<Uuid, ApiError> {
    sqlx::query_scalar(
        "SELECT b.id FROM blogs b JOIN blog_members m ON m.blog_id = b.id \
         WHERE m.account_id = $1 AND m.accepted_at IS NOT NULL AND b.deleted_at IS NULL \
         ORDER BY b.created_at LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(not_found)
}

async fn find_post(
    pool: &PgPool,
    account_id: Uuid,
    post_id: Uuid,
) -> Result<Option<PostRow>, ApiError> {
    sqlx::query_as(
        "SELECT p.id, p.slug::text AS slug, p.title, p.summary, p.content_json, \
                p.revision, p.created_at, p.updated_at \
         FROM posts p JOIN blogs b ON b.id = p.blog_id \
         JOIN blog_members m ON m.blog_id = p.blog_id \
         WHERE p.id = $1 AND m.account_id = $2 AND m.accepted_at IS NOT NULL \
           AND b.deleted_at IS NULL AND p.deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::internal)
}

async fn current_revision_or_not_found(
    pool: &PgPool,
    account_id: Uuid,
    post_id: Uuid,
) -> Result<ApiError, ApiError> {
    let revision = sqlx::query_scalar::<_, i64>(
        "SELECT p.revision FROM posts p JOIN blogs b ON b.id = p.blog_id \
         JOIN blog_members m ON m.blog_id = p.blog_id \
         WHERE p.id = $1 AND m.account_id = $2 AND m.accepted_at IS NOT NULL \
           AND b.deleted_at IS NULL AND p.deleted_at IS NULL",
    )
    .bind(post_id)
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::internal)?;
    Ok(revision.map_or_else(not_found, ApiError::stale_revision))
}

fn require_app_host(request_host: &host::RequestHost) -> Result<(), ApiError> {
    host::require_app(request_host).map_err(|_| not_found())
}

fn map_write_error(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .and_then(DatabaseError::code)
        .is_some_and(|code| code == "23505")
    {
        return ApiError::new(
            StatusCode::CONFLICT,
            "post_slug_unavailable",
            "That post address is already in use.",
        );
    }
    ApiError::internal(error)
}

fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "post_not_found",
        "That post was not found.",
    )
}

fn invalid_revision() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_revision",
        "A valid post revision is required.",
    )
}

fn invalid_content() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_post_content",
        "This post contains unsupported or malformed content.",
    )
}

impl From<PostRow> for PostResponse {
    fn from(row: PostRow) -> Self {
        Self {
            id: row.id,
            slug: row.slug,
            title: row.title,
            summary: row.summary,
            content_json: row.content_json,
            revision: row.revision,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_slug;

    #[test]
    fn post_slug_validation_covers_boundaries() {
        assert!(valid_slug("a1"));
        assert!(valid_slug("field-notes"));
        assert!(!valid_slug("a"));
        assert!(!valid_slug("-field"));
        assert!(!valid_slug("field-"));
        assert!(!valid_slug("Field"));
    }
}
