use axum::{
    Extension, Json,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use email_address::EmailAddress;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use subtle::ConstantTimeEq;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{AppState, error::ApiError, host};

const CODE_LIFETIME_SECONDS: i64 = 600;
const CODE_ATTEMPTS: i16 = 5;
const SESSION_COOKIE: &str = "nabu_session";
const SESSION_LIFETIME_DAYS: i64 = 30;

#[derive(Clone)]
pub struct AuthConfig {
    secret: Vec<u8>,
    cookie_secure: bool,
}

impl AuthConfig {
    /// Creates authentication configuration from a server-side secret.
    ///
    /// # Errors
    ///
    /// Returns an error when the secret is shorter than 32 bytes.
    pub fn new(secret: impl Into<Vec<u8>>, cookie_secure: bool) -> anyhow::Result<Self> {
        let secret = secret.into();
        anyhow::ensure!(
            secret.len() >= 32,
            "AUTH_SECRET must contain at least 32 bytes"
        );
        Ok(Self {
            secret,
            cookie_secure,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct StartEmailRequest {
    email: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct StartEmailResponse {
    pub challenge_id: Uuid,
    pub expires_in_seconds: i64,
}

#[derive(Deserialize)]
pub(crate) struct VerifyEmailRequest {
    challenge_id: Uuid,
    code: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionResponse {
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blog: Option<BlogSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csrf_token: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct AccountResponse {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub(crate) struct BlogSummary {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
}

pub(crate) struct AuthenticatedSession {
    pub account_id: Uuid,
    pub token: String,
}

#[derive(sqlx::FromRow)]
struct Challenge {
    account_id: Uuid,
    code_hash: String,
    attempts_remaining: i16,
    expires_at: OffsetDateTime,
    consumed_at: Option<OffsetDateTime>,
}

#[derive(sqlx::FromRow)]
struct SessionAccount {
    id: Uuid,
    email: String,
    display_name: Option<String>,
}

pub(crate) async fn start_email(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(request): Json<StartEmailRequest>,
) -> Result<impl IntoResponse, ApiError> {
    host::require_app(&request_host).map_err(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "The resource was not found.",
        )
    })?;

    let email = request.email.trim().to_lowercase();
    if !EmailAddress::is_valid(&email) || email.len() > 320 {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_email",
            "Enter a valid email address.",
        ));
    }

    let mut transaction = state.pool.begin().await.map_err(ApiError::internal)?;
    let account_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO accounts (id, email) VALUES ($1, $2) \
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email \
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(&email)
    .fetch_one(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;

    let throttled = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS( \
            SELECT 1 FROM login_challenges \
            WHERE account_id = $1 AND scope = 'app' \
              AND requested_at > now() - interval '1 minute' \
         )",
    )
    .bind(account_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;

    if throttled {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "code_rate_limited",
            "Wait a minute before requesting another code.",
        ));
    }

    sqlx::query(
        "UPDATE login_challenges SET consumed_at = now() \
         WHERE account_id = $1 AND scope = 'app' AND consumed_at IS NULL",
    )
    .bind(account_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;

    let challenge_id = Uuid::now_v7();
    let code = verification_code(challenge_id, &state.auth.secret);
    let code_hash = code_hash(challenge_id, &code, &state.auth.secret);

    sqlx::query(
        "INSERT INTO login_challenges \
         (id, account_id, scope, code_hash, attempts_remaining, expires_at) \
         VALUES ($1, $2, 'app', $3, $4, now() + interval '10 minutes')",
    )
    .bind(challenge_id)
    .bind(account_id)
    .bind(code_hash)
    .bind(CODE_ATTEMPTS)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;

    sqlx::query(
        "INSERT INTO email_outbox (id, recipient_account_id, template, payload) \
         VALUES ($1, $2, 'sign_in_code', jsonb_build_object('challenge_id', $3::text))",
    )
    .bind(Uuid::now_v7())
    .bind(account_id)
    .bind(challenge_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;

    transaction.commit().await.map_err(ApiError::internal)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(StartEmailResponse {
            challenge_id,
            expires_in_seconds: CODE_LIFETIME_SECONDS,
        }),
    ))
}

pub(crate) async fn verify_email(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
    Json(request): Json<VerifyEmailRequest>,
) -> Result<impl IntoResponse, ApiError> {
    host::require_app(&request_host).map_err(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "The resource was not found.",
        )
    })?;

    if request.code.len() != 6 || !request.code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_code());
    }

    let mut transaction = state.pool.begin().await.map_err(ApiError::internal)?;
    let challenge = sqlx::query_as::<_, Challenge>(
        "SELECT account_id, code_hash, attempts_remaining, expires_at, consumed_at \
         FROM login_challenges WHERE id = $1 AND scope = 'app' FOR UPDATE",
    )
    .bind(request.challenge_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(invalid_code)?;

    if challenge.consumed_at.is_some() || challenge.attempts_remaining == 0 {
        return Err(invalid_code());
    }
    if challenge.expires_at <= OffsetDateTime::now_utc() {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "code_expired",
            "That code has expired. Request a new one.",
        ));
    }

    let submitted_hash = code_hash(request.challenge_id, &request.code, &state.auth.secret);
    if !bool::from(
        challenge
            .code_hash
            .as_bytes()
            .ct_eq(submitted_hash.as_bytes()),
    ) {
        sqlx::query(
            "UPDATE login_challenges \
             SET attempts_remaining = attempts_remaining - 1 WHERE id = $1",
        )
        .bind(request.challenge_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
        transaction.commit().await.map_err(ApiError::internal)?;
        return Err(invalid_code());
    }

    sqlx::query("UPDATE login_challenges SET consumed_at = now() WHERE id = $1")
        .bind(request.challenge_id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;
    sqlx::query(
        "UPDATE accounts SET email_verified_at = COALESCE(email_verified_at, now()), \
         updated_at = now() WHERE id = $1",
    )
    .bind(challenge.account_id)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;

    let token = random_token();
    sqlx::query(
        "INSERT INTO sessions (id, account_id, scope, token_hash, expires_at) \
         VALUES ($1, $2, 'app', $3, now() + interval '30 days')",
    )
    .bind(Uuid::now_v7())
    .bind(challenge.account_id)
    .bind(token_hash(&token))
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;
    transaction.commit().await.map_err(ApiError::internal)?;

    let response =
        session_for_account(&state.pool, challenge.account_id, &token, &state.auth).await?;
    let cookie = Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .secure(state.auth.cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(Duration::days(SESSION_LIFETIME_DAYS));

    Ok((jar.add(cookie), Json(response)))
}

pub(crate) async fn session(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    jar: CookieJar,
) -> Result<Json<SessionResponse>, ApiError> {
    host::require_app(&request_host).map_err(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "The resource was not found.",
        )
    })?;

    let Some(token) = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
    else {
        return Ok(Json(signed_out()));
    };
    let Some(authenticated) = find_session(&state.pool, &token).await? else {
        return Ok(Json(signed_out()));
    };

    Ok(Json(
        session_for_account(&state.pool, authenticated.account_id, &token, &state.auth).await?,
    ))
}

pub(crate) async fn logout(
    Extension(request_host): Extension<host::RequestHost>,
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<impl IntoResponse, ApiError> {
    host::require_app(&request_host).map_err(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "The resource was not found.",
        )
    })?;
    let authenticated = require_session_and_csrf(&state, &jar, &headers).await?;

    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE token_hash = $1")
        .bind(token_hash(&authenticated.token))
        .execute(&state.pool)
        .await
        .map_err(ApiError::internal)?;

    let removal = Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .http_only(true)
        .secure(state.auth.cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(Duration::ZERO);
    Ok((jar.remove(removal), StatusCode::NO_CONTENT))
}

pub(crate) async fn require_session_and_csrf(
    state: &AppState,
    jar: &CookieJar,
    headers: &HeaderMap,
) -> Result<AuthenticatedSession, ApiError> {
    let authenticated = require_session(state, jar).await?;
    let submitted = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(invalid_csrf)?;
    let expected = csrf_token(&authenticated.token, &state.auth.secret);
    if !bool::from(submitted.as_bytes().ct_eq(expected.as_bytes())) {
        return Err(invalid_csrf());
    }
    Ok(authenticated)
}

pub(crate) async fn require_session(
    state: &AppState,
    jar: &CookieJar,
) -> Result<AuthenticatedSession, ApiError> {
    let token = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(unauthorized)?;
    find_session(&state.pool, &token)
        .await?
        .ok_or_else(unauthorized)
}

async fn find_session(
    pool: &PgPool,
    token: &str,
) -> Result<Option<AuthenticatedSession>, ApiError> {
    let account_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT account_id FROM sessions \
         WHERE token_hash = $1 AND scope = 'app' AND revoked_at IS NULL \
           AND expires_at > now()",
    )
    .bind(token_hash(token))
    .fetch_optional(pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(account_id.map(|account_id| AuthenticatedSession {
        account_id,
        token: token.to_owned(),
    }))
}

async fn session_for_account(
    pool: &PgPool,
    account_id: Uuid,
    token: &str,
    config: &AuthConfig,
) -> Result<SessionResponse, ApiError> {
    let account = sqlx::query_as::<_, SessionAccount>(
        "SELECT id, email::text AS email, display_name FROM accounts \
         WHERE id = $1 AND disabled_at IS NULL",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await
    .map_err(ApiError::internal)?;
    let blog = sqlx::query_as::<_, BlogSummary>(
        "SELECT b.id, b.slug::text AS slug, b.title \
         FROM blogs b JOIN blog_members m ON m.blog_id = b.id \
         WHERE m.account_id = $1 AND m.accepted_at IS NOT NULL \
           AND b.deleted_at IS NULL ORDER BY b.created_at LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(SessionResponse {
        authenticated: true,
        account: Some(AccountResponse {
            id: account.id,
            email: account.email,
            display_name: account.display_name,
        }),
        blog,
        csrf_token: Some(csrf_token(token, &config.secret)),
    })
}

fn signed_out() -> SessionResponse {
    SessionResponse {
        authenticated: false,
        account: None,
        blog: None,
        csrf_token: None,
    }
}

fn invalid_code() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "invalid_code",
        "That code is invalid or has already been used.",
    )
}

fn unauthorized() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "authentication_required",
        "Sign in to continue.",
    )
}

fn invalid_csrf() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "invalid_csrf_token",
        "Refresh the page and try again.",
    )
}

/// Derives the six-digit code for an email challenge.
#[must_use]
pub fn verification_code(challenge_id: Uuid, secret: &[u8]) -> String {
    let digest = hmac_bytes(secret, format!("code:{challenge_id}").as_bytes());
    let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", number % 1_000_000)
}

fn code_hash(challenge_id: Uuid, code: &str, secret: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(hmac_bytes(
        secret,
        format!("verify:{challenge_id}:{code}").as_bytes(),
    ))
}

fn csrf_token(session_token: &str, secret: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(hmac_bytes(
        secret,
        format!("csrf:{session_token}").as_bytes(),
    ))
}

fn hmac_bytes(secret: &[u8], input: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(input);
    mac.finalize().into_bytes().into()
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn token_hash(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

pub(crate) async fn challenge_email(
    transaction: &mut Transaction<'_, Postgres>,
    challenge_id: Uuid,
) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT a.email::text FROM login_challenges c \
         JOIN accounts a ON a.id = c.account_id WHERE c.id = $1",
    )
    .bind(challenge_id)
    .fetch_one(&mut **transaction)
    .await?)
}
