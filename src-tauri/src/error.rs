use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("channel not found: {0}")]
    NotFound(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("invalid stored {field} JSON: {source}")]
    InvalidStoredData {
        field: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("upstream error: {0}")]
    Upstream(#[from] anyhow::Error),
}

/// 仅在 IPC 边界序列化，业务层保留原始错误及其来源。
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        let code = match &error {
            AppError::InvalidInput(_) => "invalid_input",
            AppError::NotFound(_) => "not_found",
            AppError::Database(_) => "database_error",
            AppError::Migration(_) => "migration_error",
            AppError::InvalidStoredData { .. } => "invalid_stored_data",
            AppError::Json(_) => "serialization_error",
            AppError::Upstream(_) => "upstream_error",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}
