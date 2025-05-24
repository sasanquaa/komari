use anyhow::Result;
use ollama_rs::{Ollama, generation::completion::request::GenerationRequest};

use crate::task::Task;

#[derive(Debug, Clone, Copy)]
pub enum LanguageModel {
    Ollama,
}

#[derive(Debug)]
pub struct LanguageModelProvider {
    model: LanguageModel,
}

impl LanguageModelProvider {
    pub fn new(model: LanguageModel) -> Self {
        Self { model }
    }

    pub fn prompt(&self, prompt: String) -> Task<Result<String>> {
        match self.model {
            LanguageModel::Ollama => Task::spawn(async move {
                let ollama = Ollama::default();
                let request = GenerationRequest::new("qwen3:latest".to_string(), prompt);
                let result = ollama.generate(request).await?;
                Ok(result.response)
            }),
        }
    }
}
