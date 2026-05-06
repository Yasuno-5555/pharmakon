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

    pub fn build(&self) -> String {
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
