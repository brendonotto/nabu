use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth;

pub struct MailMessage {
    pub to: String,
    pub subject: String,
    pub text: String,
}

#[async_trait]
pub trait MailSender: Send + Sync {
    async fn send(&self, message: MailMessage) -> Result<()>;
}

pub struct SmtpMailSender {
    from: Mailbox,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpMailSender {
    /// Configures an SMTP sender.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid sender address or SMTP relay configuration.
    pub fn new(config: &SmtpConfig) -> Result<Self> {
        let from = config
            .from
            .parse()
            .context("EMAIL_FROM must be a mailbox")?;
        let mut builder = if config.tls {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
        }
        .port(config.port);
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            builder = builder.credentials(Credentials::new(username.clone(), password.clone()));
        }
        Ok(Self {
            from,
            transport: builder.build(),
        })
    }
}

#[async_trait]
impl MailSender for SmtpMailSender {
    async fn send(&self, message: MailMessage) -> Result<()> {
        let email = Message::builder()
            .from(self.from.clone())
            .to(message.to.parse()?)
            .subject(message.subject)
            .body(message.text)?;
        self.transport.send(email).await?;
        Ok(())
    }
}

pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
}

#[derive(sqlx::FromRow)]
struct OutboxMessage {
    id: Uuid,
    payload: Value,
}

/// Continuously claims and delivers pending outbox messages.
///
/// # Errors
///
/// Returns if an unrecoverable worker-loop error escapes delivery handling.
pub async fn run_worker(pool: &PgPool, sender: &dyn MailSender, auth_secret: &[u8]) -> Result<()> {
    loop {
        match deliver_one(pool, sender, auth_secret).await {
            Ok(true) => {}
            Ok(false) => tokio::time::sleep(Duration::from_secs(1)).await,
            Err(error) => {
                tracing::error!(%error, "email delivery iteration failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn deliver_one(pool: &PgPool, sender: &dyn MailSender, auth_secret: &[u8]) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    let message = sqlx::query_as::<_, OutboxMessage>(
        "SELECT id, payload FROM email_outbox \
         WHERE sent_at IS NULL AND next_attempt_at <= now() \
           AND (locked_at IS NULL OR locked_at < now() - interval '5 minutes') \
         ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(message) = message else {
        transaction.commit().await?;
        return Ok(false);
    };

    sqlx::query("UPDATE email_outbox SET locked_at = now(), attempts = attempts + 1 WHERE id = $1")
        .bind(message.id)
        .execute(&mut *transaction)
        .await?;
    let challenge_id = message
        .payload
        .get("challenge_id")
        .and_then(Value::as_str)
        .context("sign-in email is missing challenge_id")?
        .parse::<Uuid>()?;
    let recipient = auth::challenge_email(&mut transaction, challenge_id).await?;
    transaction.commit().await?;

    let code = auth::verification_code(challenge_id, auth_secret);
    let result = sender
        .send(MailMessage {
            to: recipient,
            subject: "Your Nabu sign-in code".to_owned(),
            text: format!(
                "Your Nabu sign-in code is {code}.\n\nThis code expires in 10 minutes and can be used once."
            ),
        })
        .await;

    match result {
        Ok(()) => {
            sqlx::query(
                "UPDATE email_outbox SET sent_at = now(), locked_at = NULL, last_error = NULL \
                 WHERE id = $1",
            )
            .bind(message.id)
            .execute(pool)
            .await?;
        }
        Err(error) => {
            sqlx::query(
                "UPDATE email_outbox SET locked_at = NULL, last_error = $2, \
                 next_attempt_at = now() + least(attempts, 10) * interval '30 seconds' \
                 WHERE id = $1",
            )
            .bind(message.id)
            .bind(error.to_string())
            .execute(pool)
            .await?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{MailMessage, MailSender, deliver_one};
    use crate::auth::verification_code;

    const SECRET: &[u8] = b"mail-worker-test-secret-that-is-long-enough";

    #[derive(Default)]
    struct CapturingSender {
        messages: Mutex<Vec<MailMessage>>,
    }

    #[async_trait]
    impl MailSender for CapturingSender {
        async fn send(&self, message: MailMessage) -> anyhow::Result<()> {
            self.messages.lock().unwrap().push(message);
            Ok(())
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delivers_derived_code_without_storing_it_in_outbox(pool: PgPool) {
        let account_id = Uuid::now_v7();
        let challenge_id = Uuid::now_v7();
        let outbox_id = Uuid::now_v7();
        sqlx::query("INSERT INTO accounts (id, email) VALUES ($1, 'reader@example.com')")
            .bind(account_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO login_challenges \
             (id, account_id, scope, code_hash, attempts_remaining, expires_at) \
             VALUES ($1, $2, 'app', 'not-plaintext', 5, now() + interval '10 minutes')",
        )
        .bind(challenge_id)
        .bind(account_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO email_outbox (id, recipient_account_id, template, payload) \
             VALUES ($1, $2, 'sign_in_code', jsonb_build_object('challenge_id', $3::text))",
        )
        .bind(outbox_id)
        .bind(account_id)
        .bind(challenge_id)
        .execute(&pool)
        .await
        .unwrap();

        let sender = CapturingSender::default();
        assert!(deliver_one(&pool, &sender, SECRET).await.unwrap());

        {
            let messages = sender.messages.lock().unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].to, "reader@example.com");
            assert!(
                messages[0]
                    .text
                    .contains(&verification_code(challenge_id, SECRET))
            );
        }
        let sent = sqlx::query_scalar::<_, bool>(
            "SELECT sent_at IS NOT NULL FROM email_outbox WHERE id = $1",
        )
        .bind(outbox_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(sent);
    }
}
