use anyhow::Result;
use pharmakon_common::Message;
use tiktoken_rs::cl100k_base;

/// ContextEngine manages the agent's conversation history to stay within token limits.
pub struct ContextEngine {
    max_tokens: usize,
    pinned_indices: Vec<usize>,
}

impl ContextEngine {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            pinned_indices: Vec::new(),
        }
    }

    pub fn pin_message(&mut self, index: usize) {
        if !self.pinned_indices.contains(&index) {
            self.pinned_indices.push(index);
        }
    }

    /// Prune history to stay within max_tokens limit.
    /// Priority for keeping messages:
    /// 1. System prompt (index 0)
    /// 2. Pinned messages
    /// 3. Most recent messages (last 5)
    pub async fn prune_history(&self, history: &mut Vec<Message>) -> Result<()> {
        let bpe = cl100k_base()?;

        let current_tokens = self.count_tokens(history, &bpe);
        if current_tokens <= self.max_tokens {
            return Ok(());
        }

        log::info!(
            "Context Engine: Pruning history ({} tokens > {} limit)",
            current_tokens,
            self.max_tokens
        );

        let total = history.len();
        // Indices we definitely want to keep
        let mut keep_indices: Vec<usize> = Vec::new();
        if total > 0 {
            keep_indices.push(0);
        } // System/First
        for &idx in &self.pinned_indices {
            if idx < total {
                keep_indices.push(idx);
            }
        }
        // Last 5
        let start_recent = total.saturating_sub(5);
        for i in start_recent..total {
            if !keep_indices.contains(&i) {
                keep_indices.push(i);
            }
        }
        keep_indices.sort();

        // Candidates for removal (everything else)
        let _removal_candidates: Vec<usize> =
            (0..total).filter(|i| !keep_indices.contains(i)).collect();

        // Refined removal logic:
        let mut to_keep = vec![false; total];
        for &idx in &keep_indices {
            to_keep[idx] = true;
        }

        // If we still have room, add back most recent non-must-keeps from candidates
        let mut additional_candidates: Vec<usize> = (0..total).filter(|i| !to_keep[*i]).collect();

        // Reverse to get most recent first
        additional_candidates.reverse();

        for idx in additional_candidates {
            let msg_tokens = self.count_message_tokens(&history[idx], &bpe);
            if self.count_tokens_from_map(history, &to_keep, &bpe) + msg_tokens <= self.max_tokens {
                to_keep[idx] = true;
            } else {
                break; // No more room
            }
        }

        let mut new_history = Vec::new();
        for (i, msg) in history.iter().enumerate() {
            if to_keep[i] {
                new_history.push(msg.clone());
            }
        }

        *history = new_history;
        Ok(())
    }

    fn count_tokens(&self, history: &[Message], bpe: &tiktoken_rs::CoreBPE) -> usize {
        history
            .iter()
            .map(|m| self.count_message_tokens(m, bpe))
            .sum()
    }

    fn count_tokens_from_map(
        &self,
        history: &[Message],
        to_keep: &[bool],
        bpe: &tiktoken_rs::CoreBPE,
    ) -> usize {
        history
            .iter()
            .enumerate()
            .filter(|(i, _)| to_keep[*i])
            .map(|(_, m)| self.count_message_tokens(m, bpe))
            .sum()
    }

    fn count_message_tokens(&self, msg: &Message, bpe: &tiktoken_rs::CoreBPE) -> usize {
        let content_str = match &msg.content {
            Some(c) => c.to_string(),
            None => String::new(),
        };
        let tool_calls = match &msg.tool_calls {
            Some(tc) => serde_json::to_string(tc).unwrap_or_default(),
            None => String::new(),
        };
        bpe.encode_with_special_tokens(&format!("{} {} {}", msg.role, content_str, tool_calls))
            .len()
    }
}
