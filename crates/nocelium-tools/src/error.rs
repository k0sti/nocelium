#[derive(Debug, thiserror::Error)]
pub enum NomenToolError {
    #[error("Memory error: {0}")]
    Memory(String),
}
