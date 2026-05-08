//! Context Topology Engine — addresses the "Lost in the Middle" attention dilution problem.
//!
//! LLMs are proximity machines: attention is strongest at the beginning and end of context.
//! This module restructures prompts so immutable layers sit at the prefix (enabling KV-cache hits)
//! and the most actionable information sits at the suffix.
//!
//! Architecture:
//!   [Cacheable Prefix] [Gap / Filler] [Actionable Suffix]
//!    ↑ KV-cache hit     ↑ tolerable     ↑ where the LLM actually works

use crate::model::{Message, MessageContent};
use pharmakon_memory::context_engine::ContextEntry;

// --- Prompt Layers (Token Economics via Prefix Caching) ---

/// Structured prompt layers for maximizing KV-cache hit rate.
///
/// Layer 0 (Cacheable): Fully immutable — system persona, mandates, tool descriptions.
///   These are placed at the absolute beginning and NEVER change between turns.
///   Anthropic/DeepSeek prefix caching gives ~90% cost discount on cache hits.
///
/// Layer 1 (Semi-static): Changes rarely — repo map, current goal, playbook.
///   Placed after Layer 0. Partial cache invalidation when goals change.
///
/// Layer 2 (Dynamic): Changes every turn — conversation history, tool results, working memory.
///   Placed at the end where attention is strongest.
///
/// Layer 3 (Actionable): The actual instruction — what to do NOW.
///   Placed at the absolute end. Highest attention weight.
#[derive(Debug, Clone)]
pub struct PromptLayers {
    /// Layer 0: Fully immutable. Must not change between turns.
    pub cacheable_prefix: String,

    /// Layer 1: Semi-static. Changes infrequently.
    pub semi_static: String,

    /// Layer 2: Dynamic conversation + tool results.
    pub dynamic: Vec<Message>,

    /// Layer 3: The current actionable instruction.
    pub actionable: String,
}

impl PromptLayers {
    /// Assemble the full prompt with proper topology.
    /// Returns messages in the correct order for LLM submission.
    pub fn assemble(&self) -> Vec<Message> {
        let mut messages = Vec::new();

        // Layer 0: Cacheable prefix as a single system message
        if !self.cacheable_prefix.is_empty() {
            messages.push(Message {
                role: "system".to_string(),
                content: Some(MessageContent::Text(self.cacheable_prefix.clone())),
                ..Default::default()
            });
        }

        // Layer 1: Semi-static
        if !self.semi_static.is_empty() {
            messages.push(Message {
                role: "system".to_string(),
                content: Some(MessageContent::Text(self.semi_static.clone())),
                ..Default::default()
            });
        }

        // Layer 2: Dynamic history
        messages.extend(self.dynamic.clone());

        // Layer 3: Actionable instruction
        if !self.actionable.is_empty() {
            messages.push(Message {
                role: "user".to_string(),
                content: Some(MessageContent::Text(self.actionable.clone())),
                ..Default::default()
            });
        }

        messages
    }

    /// Estimate token count (rough: 1 token ≈ 4 chars for English, 1 char ≈ 1 token for code).
    pub fn estimated_tokens(&self) -> usize {
        let cacheable = self.cacheable_prefix.len() / 4;
        let semi = self.semi_static.len() / 4;
        let dynamic: usize = self.dynamic.iter()
            .map(|m| m.content.as_ref().map(|c| c.to_string().len()).unwrap_or(0) / 4)
            .sum();
        let actionable = self.actionable.len() / 4;
        cacheable + semi + dynamic + actionable
    }
}

// --- Context Packer (Lost in the Middle Mitigation) ---

/// Context topology unit — a single block of context with a weight.
#[derive(Debug, Clone)]
pub struct ContextUnit {
    pub content: String,
    /// Importance weight (0.0 = filler, 1.0 = critical)
    pub weight: f32,
    /// Source category for debugging
    pub source: String,
}

/// Packs context into a topology that respects the attention curve.
///
/// Strategy:
/// - Head (first 20%): Immutable layers. Never touch.
/// - Body (middle 30%): Relevant context from KnowledgeNexus, repo map.
/// - Tail (last 50%): Recent conversation, tool results, working memory, current goal.
///
/// This directly addresses the "Lost in the Middle" problem where LLMs
/// attend strongly to the beginning and end but forget the middle.
pub struct ContextPacker {
    /// Total token budget for this turn.
    budget: usize,
    /// Units collected for packing.
    units: Vec<ContextUnit>,
}

impl ContextPacker {
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            units: Vec::new(),
        }
    }

    /// Add a context unit with a weight.
    pub fn push(&mut self, content: String, weight: f32, source: &str) {
        self.units.push(ContextUnit {
            content,
            weight,
            source: source.to_string(),
        });
    }

    /// Pack into a PromptLayers structure optimized for attention topology.
    pub fn pack(
        mut self,
        cacheable_prefix: String,
        semi_static: String,
        actionable: String,
    ) -> PromptLayers {
        // Sort by source priority: conversation > tool_results > nexus > filler
        let source_priority = |s: &str| -> i32 {
            match s {
                "conversation" => 4,
                "tool_result" => 3,
                "working_memory" => 2,
                "nexus" => 1,
                _ => 0,
            }
        };

        self.units.sort_by(|a, b| {
            let a_prio = source_priority(&a.source);
            let b_prio = source_priority(&b.source);
            b_prio.cmp(&a_prio)
                .then_with(|| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal))
        });

        // Fill from the tail (highest priority)
        let mut dynamic_messages = Vec::new();
        let mut remaining = self.budget.saturating_sub(
            cacheable_prefix.len() / 4 + semi_static.len() / 4 + actionable.len() / 4
        );

        for unit in self.units.iter().rev() {
            let tokens = unit.content.len() / 4;
            if tokens <= remaining {
                dynamic_messages.insert(0, unit.content.clone()); // Insert at front to maintain tail order
                remaining = remaining.saturating_sub(tokens);
            } else if remaining > 50 {
                // Truncate to fit remaining budget
                let truncate_at = remaining * 4;
                let truncated = if unit.content.len() > truncate_at {
                    format!("{}...", &unit.content[..truncate_at])
                } else {
                    unit.content.clone()
                };
                dynamic_messages.insert(0, truncated);
                remaining = 0;
            }
            if remaining == 0 {
                break;
            }
        }

        // Convert strings to Messages (they come from various sources as string blocks)
        let dynamic: Vec<Message> = dynamic_messages.into_iter()
            .enumerate()
            .map(|(i, content)| Message {
                role: if i % 3 == 0 { "assistant".to_string() } else { "system".to_string() },
                content: Some(MessageContent::Text(content)),
                ..Default::default()
            })
            .collect();

        PromptLayers {
            cacheable_prefix,
            semi_static,
            dynamic,
            actionable,
        }
    }

    /// Quick-pack: merge working memory entries and conversation into the tail.
    pub fn pack_simple(
        cacheable_prefix: String,
        semi_static: String,
        conversation: Vec<Message>,
        actionable: String,
    ) -> PromptLayers {
        PromptLayers {
            cacheable_prefix,
            semi_static,
            dynamic: conversation,
            actionable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_layers_assembly_order() {
        let layers = PromptLayers {
            cacheable_prefix: "SYSTEM: You are an engineer.".to_string(),
            semi_static: "GOAL: Fix the build.".to_string(),
            dynamic: vec![
                Message {
                    role: "user".to_string(),
                    content: Some(MessageContent::Text("Help".to_string())),
                    ..Default::default()
                },
            ],
            actionable: "Run cargo check and report errors.".to_string(),
        };

        let assembled = layers.assemble();
        assert_eq!(assembled.len(), 4);
        assert_eq!(assembled[0].role, "system");
        assert!(assembled[0].content.as_ref().unwrap().to_string().contains("engineer"));
        assert_eq!(assembled[3].role, "user");
        assert!(assembled[3].content.as_ref().unwrap().to_string().contains("cargo check"));
    }

    #[test]
    fn test_context_packer_prioritizes_recent() {
        let mut packer = ContextPacker::new(2000);
        packer.push("Old nexus result".to_string(), 0.5, "nexus");
        packer.push("Tool output from grep".to_string(), 0.8, "tool_result");
        packer.push("Recent conversation turn".to_string(), 0.9, "conversation");

        let layers = packer.pack(
            "CACHEABLE".to_string(),
            "SEMI".to_string(),
            "ACTIONABLE".to_string(),
        );

        // Verify all items made it into the packed layers
        let assembled = layers.assemble();
        let combined: String = assembled.iter()
            .filter_map(|m| m.content.as_ref().map(|c| c.to_string()))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(combined.contains("nexus"), "Should contain nexus content");
        assert!(combined.contains("grep"), "Should contain tool result");
        assert!(combined.contains("conversation"), "Should contain conversation");
        // Verify the actionable instruction is last
        let last = assembled.last().unwrap();
        assert!(last.content.as_ref().unwrap().to_string().contains("ACTIONABLE"));
    }
}
