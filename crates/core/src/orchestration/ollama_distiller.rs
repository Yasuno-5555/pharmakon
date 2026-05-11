use crate::persistence::DbSessionStore;
use crate::trajectory::{Trajectory, TrajectoryStep};
use anyhow::{anyhow, Result};
use log;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use reqwest::Client;

pub struct OllamaDistiller {
    store: Arc<DbSessionStore>,
    client: Client,
    host: String,
}

impl OllamaDistiller {
    pub fn new(store: Arc<DbSessionStore>) -> Self {
        Self {
            store,
            client: Client::new(),
            host: "http://localhost:11434".to_string(),
        }
    }

    /// Retrieve the best available local Ollama model to use as a FROM base.
    async fn resolve_base_model(&self, default_base: &str) -> String {
        let tags_url = format!("{}/api/tags", self.host);
        match self.client.get(&tags_url).send().await {
            Ok(resp) => {
                if let Ok(json) = resp.json::<Value>().await
                    && let Some(models) = json["models"].as_array() {
                        // Check if the requested default base is available
                        for m in models {
                            if let Some(name) = m["name"].as_str()
                                && name.contains(default_base) {
                                    return default_base.to_string();
                                }
                        }
                        // Fall back to the first available model if default is not found
                        if let Some(first_model) = models.first().and_then(|m| m["name"].as_str()) {
                            log::info!("Base model '{}' not found in Ollama. Using first available: '{}'", default_base, first_model);
                            return first_model.to_string();
                        }
                    }
            }
            Err(e) => {
                log::warn!("Could not connect to Ollama to resolve base models: {}. Defaulting to '{}'", e, default_base);
            }
        }
        default_base.to_string()
    }

    /// Run the distillation pipeline: compile mandates, lessons, facts, and trajectories into a Modelfile,
    /// and build the target Ollama model.
    pub async fn distill(&self, base_model_name: &str, target_model_name: &str) -> Result<String> {
        log::info!("Initializing Ollama Distillation: compiling knowledge trace...");

        // 1. Resolve base model
        let base_model = self.resolve_base_model(base_model_name).await;

        // 2. Build system prompt from workspace state
        let mut system_prompt = "You are Pharmakon Distilled — an elite, self-evolving local engineering model specialized for this workspace. You reason step-by-step and write flawless code.".to_string();

        // 2a. Append PHARMAKON.md Mandates
        let mandates_path = PathBuf::from("PHARMAKON.md");
        if mandates_path.exists()
            && let Ok(mandates) = fs::read_to_string(&mandates_path) {
                system_prompt.push_str("\n\n--- ARCHITECTURAL & ENGINEERING MANDATES ---\n");
                // Take relevant snippet or first 2000 chars to avoid prompt pollution
                system_prompt.push_str(&mandates.chars().take(2000).collect::<String>());
            }

        // 2b. Append Lessons Learned
        let lessons_path = PathBuf::from(".pharmakon/knowledge/lessons_learned.md");
        if lessons_path.exists()
            && let Ok(lessons) = fs::read_to_string(&lessons_path) {
                system_prompt.push_str("\n\n--- LESSONS LEARNED & EXPERIENCES ---\n");
                system_prompt.push_str(&lessons.chars().take(2000).collect::<String>());
            }

        // 2c. Append top workspace facts
        if let Ok(facts) = self.store.search_facts("").await
            && !facts.is_empty() {
                system_prompt.push_str("\n\n--- SPECIFIC WORKSPACE FACTS ---\n");
                for f in facts.iter().take(10) {
                    if let Some(content) = f["content"].as_str() {
                        system_prompt.push_str(&format!("- {}\n", content));
                    }
                }
            }

        // 3. Generate the Modelfile headers
        let mut modelfile = format!("FROM {}\n\n", base_model);
        modelfile.push_str("PARAMETER temperature 0.2\n");
        modelfile.push_str("PARAMETER num_ctx 8192\n\n");
        modelfile.push_str(&format!("SYSTEM \"\"\"{}\"\"\"\n\n", system_prompt.replace("\"", "\\\"")));

        // 4. Extract trajectories and convert to MESSAGE commands (few-shot training)
        log::info!("Extracting historical trajectories for agentic sequence alignment...");
        if let Ok(trajectories) = self.store.load_all_trajectories(30).await {
            let mut sample_count = 0;
            for t in trajectories {
                let (user_query, assistant_response) = self.format_trajectory_to_dialogue(&t);
                if !user_query.is_empty() && !assistant_response.is_empty() {
                    // Inject dialogue into Modelfile
                    modelfile.push_str(&format!("MESSAGE user \"\"\"{}\"\"\"\n", user_query.replace("\"", "\\\"")));
                    modelfile.push_str(&format!("MESSAGE assistant \"\"\"{}\"\"\"\n\n", assistant_response.replace("\"", "\\\"")));
                    sample_count += 1;
                }
            }
            log::info!("Compiled {} high-fidelity agentic trajectories into the training Modelfile.", sample_count);
        }

        // 5. Save the compiled Modelfile locally for debuggability
        let distill_dir = PathBuf::from(".pharmakon/distill");
        let _ = fs::create_dir_all(&distill_dir);
        let modelfile_path = distill_dir.join("Modelfile");
        fs::write(&modelfile_path, &modelfile)?;
        log::info!("Modelfile saved to: {}", modelfile_path.display());

        // 6. Request Ollama to create the custom model
        log::info!("Submitting build request to Ollama daemon (target: {})...", target_model_name);
        let create_url = format!("{}/api/create", self.host);
        let response = self.client.post(&create_url)
            .json(&json!({
                "name": target_model_name,
                "modelfile": modelfile,
                "stream": false
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Ollama build error: {}", err_text));
        }

        let result_json: Value = response.json().await?;
        if result_json["status"].as_str() == Some("success") {
            log::info!("🎉 Ollama background distillation completed successfully! Target model '{}' is now compiled and ready.", target_model_name);
            Ok(target_model_name.to_string())
        } else {
            Err(anyhow!("Ollama build reported unexpected status: {:?}", result_json))
        }
    }

    /// Formats a full step-by-step trajectory into a single compressed User/Assistant QA pair
    /// to teach the model how to reason and structure actions inside Pharmakon.
    fn format_trajectory_to_dialogue(&self, trajectory: &Trajectory) -> (String, String) {
        let mut user_query = String::new();
        let mut assistant_steps = Vec::new();

        for step in &trajectory.steps {
            match step {
                TrajectoryStep::Intent { goal, .. } => {
                    if user_query.is_empty() {
                        user_query = goal.clone();
                    }
                }
                TrajectoryStep::Thought { content, .. } => {
                    assistant_steps.push(format!("[Thought]\n{}", content));
                }
                TrajectoryStep::Action { tool, args, .. } => {
                    assistant_steps.push(format!("[Action]\nTool: {}\nArgs: {}", tool, args));
                }
                TrajectoryStep::Observation { result, .. } => {
                    // Truncate overly long tool output in messages to prevent context overflow
                    let mut truncated = result.chars().take(500).collect::<String>();
                    if result.len() > 500 {
                        truncated.push_str("\n...[Truncated for distillation]...");
                    }
                    assistant_steps.push(format!("[Observation]\n{}", truncated));
                }
                TrajectoryStep::Response { content, .. } => {
                    assistant_steps.push(format!("[Response]\n{}", content));
                }
            }
        }

        (user_query, assistant_steps.join("\n\n"))
    }
}
