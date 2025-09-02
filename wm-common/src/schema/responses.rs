use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceResponse {
    pub eps: usize,
}
