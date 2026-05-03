use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Commitment {
    pub id: Uuid,
    pub task: String,
    pub created_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub status: CommitmentStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CommitmentStatus {
    Pending,
    Completed,
    Abandoned,
}

pub struct CommitmentManager {
    pub commitments: Vec<Commitment>,
}

impl CommitmentManager {
    pub fn new() -> Self {
        Self { commitments: Vec::new() }
    }

    pub fn add_commitment(&mut self, task: String, deadline: Option<DateTime<Utc>>) -> Uuid {
        let commitment = Commitment {
            id: Uuid::new_v4(),
            task,
            created_at: Utc::now(),
            deadline,
            status: CommitmentStatus::Pending,
        };
        let id = commitment.id;
        self.commitments.push(commitment);
        id
    }

    pub fn mark_completed(&mut self, id: Uuid) {
        if let Some(c) = self.commitments.iter_mut().find(|c| c.id == id) {
            c.status = CommitmentStatus::Completed;
        }
    }
}
