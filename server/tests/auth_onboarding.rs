use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use nabu_server::{
    app,
    auth::{AuthConfig, verification_code},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &[u8] = b"integration-test-secret-that-is-long-enough";

fn test_app(pool: PgPool) -> Router {
    app(
        pool,
        "nabu.test".to_owned(),
        AuthConfig::new(SECRET.to_vec(), false).unwrap(),
    )
}

async fn request_json(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    cookie: Option<&str>,
    csrf: Option<&str>,
) -> Response<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "app.nabu.test")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(csrf) = csrf {
        builder = builder.header("x-csrf-token", csrf);
    }

    app.clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn response_json(response: Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn start_sign_in(app: &Router, email: &str) -> Uuid {
    let response = request_json(
        app,
        "POST",
        "/api/v1/auth/email/start",
        json!({ "email": email }),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    response_json(response).await["challenge_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn sign_in(app: &Router, email: &str) -> (String, String) {
    let challenge_id = start_sign_in(app, email).await;
    let response = request_json(
        app,
        "POST",
        "/api/v1/auth/email/verify",
        json!({
            "challenge_id": challenge_id,
            "code": verification_code(challenge_id, SECRET),
        }),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(set_cookie.contains("Path=/"));
    assert!(set_cookie.contains("Max-Age=2592000"));
    let cookie = set_cookie.split(';').next().unwrap().to_owned();
    let body = response_json(response).await;
    let csrf = body["csrf_token"].as_str().unwrap().to_owned();
    (cookie, csrf)
}

#[sqlx::test(migrations = "./migrations")]
async fn author_session_persists_and_creates_one_blog(pool: PgPool) {
    let app = test_app(pool.clone());
    let (cookie, csrf) = sign_in(&app, "author@example.com").await;
    let raw_token = cookie.split_once('=').unwrap().1;
    let stored_hash: String = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_hash.len(), 43);
    assert_ne!(stored_hash, raw_token);

    let missing_csrf = request_json(
        &app,
        "POST",
        "/api/v1/blog",
        json!({ "title": "Field Notes", "slug": "field-notes" }),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

    let created = request_json(
        &app,
        "POST",
        "/api/v1/blog",
        json!({ "title": "Field Notes", "slug": "field-notes" }),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let session = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/session")
                .header(header::HOST, "app.nabu.test")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(session.status(), StatusCode::OK);
    let body = response_json(session).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["blog"]["slug"], "field-notes");

    let logout = request_json(
        &app,
        "POST",
        "/api/v1/auth/logout",
        json!({}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert!(
        logout
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );

    let signed_out_session = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/session")
                .header(header::HOST, "app.nabu.test")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signed_out_session.status(), StatusCode::OK);
    assert_eq!(
        response_json(signed_out_session).await["authenticated"],
        false
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn verification_enforces_attempt_and_expiry_boundaries(pool: PgPool) {
    let app = test_app(pool.clone());
    let challenge_id = start_sign_in(&app, "attempts@example.com").await;
    let valid_code = verification_code(challenge_id, SECRET);
    let wrong_code = if valid_code == "000000" {
        "999999"
    } else {
        "000000"
    };

    for expected_attempts in (0_i16..5).rev() {
        let response = request_json(
            &app,
            "POST",
            "/api/v1/auth/email/verify",
            json!({ "challenge_id": challenge_id, "code": wrong_code }),
            None,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let remaining = sqlx::query_scalar::<_, i16>(
            "SELECT attempts_remaining FROM login_challenges WHERE id = $1",
        )
        .bind(challenge_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, expected_attempts);
    }

    let locked_out = request_json(
        &app,
        "POST",
        "/api/v1/auth/email/verify",
        json!({ "challenge_id": challenge_id, "code": valid_code }),
        None,
        None,
    )
    .await;
    assert_eq!(locked_out.status(), StatusCode::UNAUTHORIZED);

    let expired_id = start_sign_in(&app, "expired@example.com").await;
    sqlx::query(
        "UPDATE login_challenges SET expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(expired_id)
    .execute(&pool)
    .await
    .unwrap();
    let expired = request_json(
        &app,
        "POST",
        "/api/v1/auth/email/verify",
        json!({
            "challenge_id": expired_id,
            "code": verification_code(expired_id, SECRET),
        }),
        None,
        None,
    )
    .await;
    assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_json(expired).await["error"]["code"],
        "code_expired"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn throttles_code_requests_and_rejects_duplicate_blog_slugs(pool: PgPool) {
    let app = test_app(pool);
    start_sign_in(&app, "rate-limit@example.com").await;
    let throttled = request_json(
        &app,
        "POST",
        "/api/v1/auth/email/start",
        json!({ "email": "rate-limit@example.com" }),
        None,
        None,
    )
    .await;
    assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);

    let (first_cookie, first_csrf) = sign_in(&app, "first@example.com").await;
    let first = request_json(
        &app,
        "POST",
        "/api/v1/blog",
        json!({ "title": "First", "slug": "shared-name" }),
        Some(&first_cookie),
        Some(&first_csrf),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let (second_cookie, second_csrf) = sign_in(&app, "second@example.com").await;
    let duplicate = request_json(
        &app,
        "POST",
        "/api/v1/blog",
        json!({ "title": "Second", "slug": "shared-name" }),
        Some(&second_cookie),
        Some(&second_csrf),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(duplicate).await["error"]["code"],
        "blog_slug_unavailable"
    );
}
