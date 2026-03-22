use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Nomen connection failed: {0}")]
    Connection(String),

    #[error("Nomen API error ({code}): {message}")]
    Api { code: String, message: String },

    #[error("Deserialization failed: {0}")]
    Deserialize(String),
}
