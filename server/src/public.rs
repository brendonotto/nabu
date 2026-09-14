use std::fmt::Write as _;

use askama::Template;
use axum::{
    Extension,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use sqlx::FromRow;
use time::{Month, OffsetDateTime, format_description::well_known::Rfc2822};

use crate::{AppState, host::RequestHost};

#[derive(FromRow)]
struct BlogRow {
    id: uuid::Uuid,
    title: String,
}

#[derive(FromRow)]
struct PostRow {
    slug: String,
    title: String,
    summary: Option<String>,
    published_at: OffsetDateTime,
}

#[derive(FromRow)]
struct PostPageRow {
    blog_title: String,
    slug: String,
    title: String,
    summary: Option<String>,
    rendered_html: String,
    published_at: OffsetDateTime,
}

struct PostSummary {
    title: String,
    summary: String,
    url: String,
    published_date: String,
}

#[derive(Template)]
#[template(path = "public_blog.html")]
struct BlogTemplate {
    blog_title: String,
    canonical_url: String,
    feed_url: String,
    posts: Vec<PostSummary>,
}

#[derive(Template)]
#[template(path = "public_post.html")]
struct PostTemplate {
    blog_title: String,
    blog_url: String,
    title: String,
    description: String,
    canonical_url: String,
    published_date: String,
    published_datetime: String,
    rendered_html: String,
}

pub(crate) async fn blog(
    Extension(request_host): Extension<RequestHost>,
    State(state): State<AppState>,
) -> Result<Html<String>, StatusCode> {
    let slug = blog_slug(&request_host)?;
    let blog = find_blog(&state, slug).await?;
    let posts = public_posts(&state, blog.id).await?;
    if posts.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let base_url = base_url(slug, &state.root_domain);
    let template = BlogTemplate {
        blog_title: blog.title,
        canonical_url: format!("{base_url}/"),
        feed_url: format!("{base_url}/feed.xml"),
        posts: posts
            .into_iter()
            .map(|post| PostSummary {
                title: post.title,
                summary: post.summary.unwrap_or_default(),
                url: format!("{base_url}/{}", post.slug),
                published_date: display_date(post.published_at),
            })
            .collect(),
    };
    template
        .render()
        .map(Html)
        .map_err(|error| internal_error(error, "public blog rendering failed"))
}

pub(crate) async fn post(
    Extension(request_host): Extension<RequestHost>,
    State(state): State<AppState>,
    Path(post_slug): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let slug = blog_slug(&request_host)?;
    let row = sqlx::query_as::<_, PostPageRow>(
        "SELECT b.title AS blog_title, p.slug::text AS slug, p.title, p.summary, p.rendered_html, \
                p.published_at \
         FROM posts p JOIN blogs b ON b.id = p.blog_id \
         WHERE b.slug = $1 AND p.slug = $2 AND b.deleted_at IS NULL \
           AND p.deleted_at IS NULL AND p.publication_status = 'published' \
           AND p.visibility = 'public'",
    )
    .bind(slug)
    .bind(&post_slug)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| internal_error(error, "public post lookup failed"))?
    .ok_or(StatusCode::NOT_FOUND)?;
    let base_url = base_url(slug, &state.root_domain);
    let canonical_url = format!("{base_url}/{}", row.slug);
    let template = PostTemplate {
        blog_title: row.blog_title,
        blog_url: format!("{base_url}/"),
        title: row.title,
        description: row.summary.unwrap_or_default(),
        canonical_url,
        published_date: display_date(row.published_at),
        published_datetime: row.published_at.to_string(),
        rendered_html: row.rendered_html,
    };
    template
        .render()
        .map(Html)
        .map_err(|error| internal_error(error, "public post rendering failed"))
}

pub(crate) async fn feed(
    Extension(request_host): Extension<RequestHost>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let slug = blog_slug(&request_host)?;
    let blog = find_blog(&state, slug).await?;
    let posts = public_posts(&state, blog.id).await?;
    if posts.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let base_url = base_url(slug, &state.root_domain);
    let mut items = String::new();
    for post in posts {
        let url = format!("{base_url}/{}", post.slug);
        let published_at = post
            .published_at
            .format(&Rfc2822)
            .map_err(|error| internal_error(error, "RSS publication date formatting failed"))?;
        write!(
            items,
            "<item><title>{}</title><link>{}</link><guid>{}</guid><description>{}</description><pubDate>{}</pubDate></item>",
            xml(&post.title),
            xml(&url),
            xml(&url),
            xml(post.summary.as_deref().unwrap_or("")),
            published_at,
        )
        .expect("writing RSS to a String cannot fail");
    }
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><rss version=\"2.0\"><channel><title>{}</title><link>{}/</link><description>Public posts from {}</description>{}</channel></rss>",
        xml(&blog.title),
        xml(&base_url),
        xml(&blog.title),
        items,
    );
    Ok(document_response(
        "application/rss+xml; charset=utf-8",
        body,
    ))
}

pub(crate) async fn sitemap(
    Extension(request_host): Extension<RequestHost>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let slug = blog_slug(&request_host)?;
    let blog = find_blog(&state, slug).await?;
    let posts = public_posts(&state, blog.id).await?;
    if posts.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let base_url = base_url(slug, &state.root_domain);
    let mut urls = format!("<url><loc>{}/</loc></url>", xml(&base_url));
    for post in posts {
        write!(
            urls,
            "<url><loc>{}/{}</loc><lastmod>{}</lastmod></url>",
            xml(&base_url),
            xml(&post.slug),
            post.published_at.date(),
        )
        .expect("writing a sitemap to a String cannot fail");
    }
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">{urls}</urlset>"
    );
    Ok(document_response("application/xml; charset=utf-8", body))
}

pub(crate) async fn robots(
    Extension(request_host): Extension<RequestHost>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let slug = blog_slug(&request_host)?;
    let blog = find_blog(&state, slug).await?;
    let has_public_posts = !public_posts(&state, blog.id).await?.is_empty();
    let body = if has_public_posts {
        format!(
            "User-agent: *\nAllow: /\nSitemap: {}/sitemap.xml\n",
            base_url(slug, &state.root_domain)
        )
    } else {
        "User-agent: *\nDisallow: /\n".to_owned()
    };
    Ok(document_response("text/plain; charset=utf-8", body))
}

async fn find_blog(state: &AppState, slug: &str) -> Result<BlogRow, StatusCode> {
    sqlx::query_as("SELECT id, title FROM blogs WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(&state.pool)
        .await
        .map_err(|error| internal_error(error, "public blog lookup failed"))?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn public_posts(state: &AppState, blog_id: uuid::Uuid) -> Result<Vec<PostRow>, StatusCode> {
    sqlx::query_as(
        "SELECT slug::text AS slug, title, summary, published_at \
         FROM posts WHERE blog_id = $1 AND deleted_at IS NULL \
           AND publication_status = 'published' AND visibility = 'public' \
         ORDER BY published_at DESC, id DESC",
    )
    .bind(blog_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| internal_error(error, "public post listing failed"))
}

fn blog_slug(host: &RequestHost) -> Result<&str, StatusCode> {
    match host {
        RequestHost::Blog(slug) => Ok(slug),
        RequestHost::App => Err(StatusCode::NOT_FOUND),
    }
}

fn base_url(slug: &str, root_domain: &str) -> String {
    format!("https://{slug}.{root_domain}")
}

fn display_date(value: OffsetDateTime) -> String {
    let month = match value.month() {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    };
    format!("{month} {}, {}", value.day(), value.year())
}

fn xml(value: &str) -> String {
    html_escape::encode_text(value).into_owned()
}

fn document_response(content_type: &'static str, body: String) -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
        body,
    )
        .into_response()
}

fn internal_error(error: impl std::fmt::Display, message: &'static str) -> StatusCode {
    tracing::error!(%error, "{message}");
    StatusCode::INTERNAL_SERVER_ERROR
}
