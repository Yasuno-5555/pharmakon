pub mod causal_graph;
pub mod commitment;
pub mod compactor;
pub mod context_engine;
pub mod episodic_memory;
pub mod fact_memory;
pub mod graph;
pub mod procedural_memory;
pub mod semantic_search;
pub mod vector_store;
pub mod weaver;

pub use causal_graph::{CausalEdge, CausalEdgeType, CausalGraph, CausalNode, CausalNodeType};
pub use commitment::Commitment;
pub use fact_memory::{Belief, BeliefSystem};
pub use procedural_memory::{ProceduralStore, Procedure};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RagStrategy {
    InitialContext { top_k: usize },
    ToolCall,
    Hybrid { initial_top_k: usize },
    DeepResearch { max_depth: u8, beam_width: usize },
    Disabled,
}
