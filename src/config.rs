//! Configuration from environment variables. In development a `.env` file is
//! loaded first (see `.env.example`); variables already set take precedence.

use crate::error::AppError;

#[derive(Clone)]
pub struct Config {
    /// PostgreSQL connection URL. Holds credentials: never log it.
    pub database_url: String,
}

// Hand-written so the database URL (and its password) can never end up in a log.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config").finish_non_exhaustive()
    }
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        Self::from_database_url(std::env::var("DATABASE_URL").ok())
    }

    fn from_database_url(database_url: Option<String>) -> Result<Self, AppError> {
        let database_url = database_url
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .ok_or_else(|| AppError::Config("DATABASE_URL is not set".into()))?;
        if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
            return Err(AppError::Config(
                "DATABASE_URL must be a postgres:// or postgresql:// URL".into(),
            ));
        }
        Ok(Self { database_url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_url_is_required() {
        assert!(matches!(
            Config::from_database_url(None),
            Err(AppError::Config(_))
        ));
        assert!(matches!(
            Config::from_database_url(Some("  ".into())),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn database_url_must_be_postgres() {
        let error = Config::from_database_url(Some("mysql://localhost/notes".into())).unwrap_err();
        assert!(error.to_string().contains("postgres://"));
        let config =
            Config::from_database_url(Some("postgres://u:secret@localhost/notes".into())).unwrap();
        assert_eq!(config.database_url, "postgres://u:secret@localhost/notes");
    }

    #[test]
    fn debug_output_hides_the_url() {
        let config =
            Config::from_database_url(Some("postgres://u:secret@localhost/notes".into())).unwrap();
        assert!(!format!("{config:?}").contains("secret"));
    }
}
