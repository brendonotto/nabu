use std::{env, net::SocketAddr};

use anyhow::{Context, Result};

use crate::{auth::AuthConfig, mail::SmtpConfig};

pub struct Config {
    pub bind_address: SocketAddr,
    pub database_url: String,
    pub root_domain: String,
    pub auth: AuthConfig,
}

impl Config {
    /// Loads required server configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error when required values are absent or invalid.
    pub fn from_env() -> Result<Self> {
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let bind_address = env::var("BIND_ADDRESS")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
            .parse()
            .context("BIND_ADDRESS must be a socket address")?;
        let root_domain = env::var("ROOT_DOMAIN").context("ROOT_DOMAIN must be set")?;
        let auth_secret = env::var("AUTH_SECRET").context("AUTH_SECRET must be set")?;
        let cookie_secure = env_bool("COOKIE_SECURE", true)?;

        Ok(Self {
            bind_address,
            database_url,
            root_domain,
            auth: AuthConfig::new(auth_secret, cookie_secure)?,
        })
    }
}

pub struct WorkerConfig {
    pub database_url: String,
    pub auth_secret: String,
    pub smtp: SmtpConfig,
}

impl WorkerConfig {
    /// Loads worker and SMTP configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error when required values are absent or malformed.
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
            auth_secret: env::var("AUTH_SECRET").context("AUTH_SECRET must be set")?,
            smtp: SmtpConfig {
                host: env::var("SMTP_HOST").context("SMTP_HOST must be set")?,
                port: env::var("SMTP_PORT")
                    .unwrap_or_else(|_| "587".to_owned())
                    .parse()
                    .context("SMTP_PORT must be a port number")?,
                tls: env_bool("SMTP_TLS", true)?,
                username: env::var("SMTP_USERNAME")
                    .ok()
                    .filter(|value| !value.is_empty()),
                password: env::var("SMTP_PASSWORD")
                    .ok()
                    .filter(|value| !value.is_empty()),
                from: env::var("EMAIL_FROM").context("EMAIL_FROM must be set")?,
            },
        })
    }
}

fn env_bool(name: &str, default: bool) -> Result<bool> {
    match env::var(name) {
        Ok(value) if value.eq_ignore_ascii_case("true") => Ok(true),
        Ok(value) if value.eq_ignore_ascii_case("false") => Ok(false),
        Ok(_) => anyhow::bail!("{name} must be true or false"),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error).with_context(|| format!("could not read {name}")),
    }
}
