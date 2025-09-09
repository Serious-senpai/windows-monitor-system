use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceResponse {
    pub emit_eps: usize,
    pub receive_eps: usize,
}
