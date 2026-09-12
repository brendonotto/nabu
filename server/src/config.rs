use std::{env, net::SocketAddr};

use anyhow::{Context, Result};

pub struct Config {
    pub bind_address: SocketAddr,
    pub database_url: String,
}

impl Config {
    /// Loads required server configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error when `DATABASE_URL` is absent or `BIND_ADDRESS` is not
    /// a valid socket address.
    pub fn from_env() -> Result<Self> {
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let bind_address = env::var("BIND_ADDRESS")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
            .parse()
            .context("BIND_ADDRESS must be a socket address")?;

        Ok(Self {
            bind_address,
            database_url,
        })
    }
}
