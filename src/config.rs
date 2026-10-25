//! Configuration from environment variables. In development a `.env` file is
//! loaded first (see `.env.example`); variables already set take precedence.

use crate::error::AppError;

#[derive(Clone)]
pub struct Config {
    /// PostgreSQL connection URL. Holds credentials: never log it.
    pub database_url: String,
    /// Apply pending migrations at startup (default). Turn off when the server
    /// runs as a least-privilege user and migrations are applied separately.
    pub migrate_on_startup: bool,
}

// Hand-written so the database URL (and its password) can never end up in a log.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("migrate_on_startup", &self.migrate_on_startup)
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        Self::from_values(
            std::env::var("DATABASE_URL").ok(),
            std::env::var("MIGRATE_ON_STARTUP").ok(),
        )
    }

    fn from_values(
        database_url: Option<String>,
        migrate_on_startup: Option<String>,
    ) -> Result<Self, AppError> {
        let database_url = database_url
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .ok_or_else(|| AppError::Config("DATABASE_URL is not set".into()))?;
        if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
            return Err(AppError::Config(
                "DATABASE_URL must be a postgres:// or postgresql:// URL".into(),
            ));
        }
        let migrate_on_startup = match migrate_on_startup.as_deref().map(str::trim) {
            None | Some("") | Some("true") | Some("1") => true,
            Some("false") | Some("0") => false,
            Some(_) => {
                return Err(AppError::Config(
                    "MIGRATE_ON_STARTUP must be true or false".into(),
                ));
            }
        };
        Ok(Self {
            database_url,
            migrate_on_startup,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "postgres://u:secret@localhost/notes";

    #[test]
    fn database_url_is_required() {
        assert!(matches!(
            Config::from_values(None, None),
            Err(AppError::Config(_))
        ));
        assert!(matches!(
            Config::from_values(Some("  ".into()), None),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn database_url_must_be_postgres() {
        let error = Config::from_values(Some("mysql://localhost/notes".into()), None).unwrap_err();
        assert!(error.to_string().contains("postgres://"));
        assert_eq!(
            Config::from_values(Some(URL.into()), None)
                .unwrap()
                .database_url,
            URL
        );
    }

    #[test]
    fn migrations_run_at_startup_unless_turned_off() {
        let migrate = |value: Option<&str>| {
            Config::from_values(Some(URL.into()), value.map(String::from))
                .map(|c| c.migrate_on_startup)
        };
        assert!(migrate(None).unwrap());
        assert!(migrate(Some("true")).unwrap());
        assert!(!migrate(Some("false")).unwrap());
        assert!(!migrate(Some("0")).unwrap());
        assert!(migrate(Some("maybe")).is_err());
    }

    #[test]
    fn debug_output_hides_the_url() {
        let config = Config::from_values(Some(URL.into()), None).unwrap();
        assert!(!format!("{config:?}").contains("secret"));
    }
}
