use serde::{Deserialize, Serialize};
use specta::Type;

use super::{ErrorKind, MutationKind, OptimismLevel};

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct SubmitPayload {
    pub kind: MutationKind,
    pub target_type: String,
    pub target_id: String,
    pub idempotency_key: String,
    pub input_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PendingOverlay {
    pub mutation_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct PendingMutationView {
    pub id: String,
    pub account_id: String,
    pub kind: MutationKind,
    pub optimism: OptimismLevel,
    pub target_type: String,
    pub target_id: String,
    pub status: String,
    pub retries: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_error: Option<String>,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct HardConflictDiff {
    pub summary: String,
    pub local_body: Option<String>,
    pub server_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MutationEvent {
    Submitted {
        mutation: PendingMutationView,
    },
    Applied {
        mutation_id: String,
    },
    Reconciled {
        mutation_id: String,
    },
    Failed {
        mutation_id: String,
        error_kind: ErrorKind,
        retryable: bool,
        hard_conflict: Option<HardConflictDiff>,
    },
    RolledBack {
        mutation_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct SubmittedMutation {
    pub mutation_id: String,
    pub deduped: bool,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct DrainSummary {
    pub processed: usize,
    pub applied: usize,
    pub reconciled: usize,
    pub failed: usize,
    pub rolled_back: usize,
    pub retried: usize,
}
