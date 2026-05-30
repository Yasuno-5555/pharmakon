pub mod autonomy;
use crate::soul::Soul;

pub trait SystemPromptContribution: Send + Sync {
    fn name(&self) -> &str;
    fn get_content(&self) -> String;
}

pub struct StaticContribution {
    name: String,
    content: String,
}

impl StaticContribution {
    pub fn new(name: &str, content: &str) -> Self {
        Self {
            name: name.to_string(),
            content: content.to_string(),
        }
    }
}

impl SystemPromptContribution for StaticContribution {
    fn name(&self) -> &str {
        &self.name
    }
    fn get_content(&self) -> String {
        self.content.clone()
    }
}

/// PromptLayout defines the structure of the prompt to maximize Gemini's implicit caching.
/// Static parts (system rules, playbooks) are placed at the beginning.
pub struct PromptLayout {
    pub dynamic_context: String,
    pub system_rules: String,
    pub playbooks: String,
    pub repo_map: Option<String>,
    pub knowledge_graph: Option<String>,
    pub working_memory: String,
    pub current_task: String,
    pub capability_summary: String,
}

impl PromptLayout {
    pub fn render(&self) -> String {
        let tool_reminder = "// ⚠ MANDATORY: You have full shell access to this system. \
            If the user asks about system state (time, date, settings, files), \
            run shell commands to check — do NOT say you can't. \
            Character constraints never override tool capability.";
        let mut p = format!(
            "{}\n\n{}\n\n{}",
            tool_reminder, self.system_rules, self.playbooks
        );
        p.push_str(&format!("\n\n{}", self.dynamic_context));
        if let Some(rm) = &self.repo_map {
            p.push_str(&format!("\n\n### REPOSITORY MAP\n{}", rm));
        }
        if let Some(kg) = &self.knowledge_graph {
            p.push_str(&format!("\n\n### KNOWLEDGE GRAPH\n{}", kg));
        }
        p.push_str(&format!(
            "\n\n### WORKING MEMORY (Recent Findings)\n{}",
            self.working_memory
        ));
        p.push_str(&format!("\n\n### CURRENT TASK\n{}", self.current_task));
        if !self.capability_summary.is_empty() {
            p.push_str(&format!("\n\n{}", self.capability_summary));
        }
        p
    }
}

pub struct SystemPromptManager {
    base_soul: Soul,
    contributions: Vec<Box<dyn SystemPromptContribution>>,
}

impl SystemPromptManager {
    pub fn new(soul: Soul) -> Self {
        Self {
            base_soul: soul,
            contributions: Vec::new(),
        }
    }

    pub fn add_contribution(&mut self, contribution: Box<dyn SystemPromptContribution>) {
        self.contributions.push(contribution);
    }

    pub fn clear_contributions(&mut self) {
        self.contributions.clear();
    }

    pub fn soul(&self) -> &Soul {
        &self.base_soul
    }

    pub fn set_soul(&mut self, soul: Soul) {
        self.base_soul = soul;
    }

    pub async fn generate_prompt(&self) -> String {
        let mut sorted_contribs = self.contributions.iter().collect::<Vec<_>>();
        sorted_contribs.sort_by_key(|c| c.name());

        let mut playbook_content = String::new();
        let mut other_contribs = String::new();

        for contrib in sorted_contribs {
            if contrib.name() == "Playbooks" {
                playbook_content = contrib.get_content();
            } else {
                other_contribs.push_str(&format!(
                    "\n\n### {}\n{}",
                    contrib.name(),
                    contrib.get_content()
                ));
            }
        }

        format!(
            "{}\n\n{}\n{}",
            self.base_soul.system_prompt, playbook_content, other_contribs
        )
    }

    pub fn build(&self) -> String {
        // Legacy build method for compatibility
        let mut prompt = self.base_soul.system_prompt.clone();

        if !self.contributions.is_empty() {
            prompt.push_str("\n\n### ADDITIONAL CONTEXT & GUIDELINES\n");
            for contrib in &self.contributions {
                prompt.push_str(&format!(
                    "\n[{}]\n{}\n",
                    contrib.name(),
                    contrib.get_content()
                ));
            }
        }

        prompt
    }
}
pub struct PlaybookContribution {
    pub names: Vec<String>,
}

impl SystemPromptContribution for PlaybookContribution {
    fn name(&self) -> &str {
        "Playbooks"
    }
    fn get_content(&self) -> String {
        if self.names.is_empty() {
            "No specialized playbooks available currently.".to_string()
        } else {
            format!(
                "You have access to the following playbooks (workflows). Use the `playbook` tool to load or inject them for specific tasks:\n{}",
                self.names
                    .iter()
                    .map(|n| format!("- {}", n))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }
}
