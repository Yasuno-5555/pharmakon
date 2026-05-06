pub mod commitment;
pub mod compactor;
pub mod context_engine;
pub mod fact_memory;
pub mod graph;
pub mod semantic_search;
pub mod vector_store;
pub mod weaver;

pub use commitment::Commitment;
pub use fact_memory::{Fact, FactMemory};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RagStrategy {
    InitialContext { top_k: usize },
    ToolCall,
    Hybrid { initial_top_k: usize },
    Disabled,
}
