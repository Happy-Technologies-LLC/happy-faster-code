use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Anthropic,
    OpenAI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub provider: ProviderKind,
    pub model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_iterations: usize,
}

impl AgentConfig {
    /// Load config from env vars, with optional TOML fallback.
    /// Priority: env vars > TOML > defaults.
    /// If no API key is found, returns Ok with empty api_key (caller should prompt).
    pub fn load(repo_path: Option<&str>) -> anyhow::Result<Self> {
        // Start with defaults
        let mut config = Self::default();

        // Try loading .happy/agent.toml
        if let Some(repo) = repo_path {
            let toml_path = Path::new(repo).join(".happy").join("agent.toml");
            if toml_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&toml_path) {
                    if let Ok(file_config) = toml::from_str::<FileConfig>(&contents) {
                        if let Some(p) = file_config.provider {
                            config.provider = p;
                        }
                        if let Some(m) = file_config.model {
                            config.model = m;
                        }
                        if let Some(k) = file_config.api_key {
                            config.api_key = k;
                        }
                        if let Some(b) = file_config.api_base {
                            config.api_base = Some(b);
                        }
                        if let Some(t) = file_config.max_tokens {
                            config.max_tokens = t;
                        }
                        if let Some(t) = file_config.temperature {
                            config.temperature = t;
                        }
                        if let Some(i) = file_config.max_iterations {
                            config.max_iterations = i;
                        }
                    }
                }
            }
        }

        // Env var overrides
        if let Ok(provider) = std::env::var("HAPPY_PROVIDER") {
            config.provider = match provider.to_lowercase().as_str() {
                "openai" => ProviderKind::OpenAI,
                _ => ProviderKind::Anthropic,
            };
        }

        if let Ok(model) = std::env::var("HAPPY_MODEL") {
            config.model = model;
        }

        if let Ok(max_tokens) = std::env::var("HAPPY_MAX_TOKENS") {
            if let Ok(mt) = max_tokens.parse() {
                config.max_tokens = mt;
            }
        }

        if let Ok(temp) = std::env::var("HAPPY_TEMPERATURE") {
            if let Ok(t) = temp.parse() {
                config.temperature = t;
            }
        }

        if let Ok(iters) = std::env::var("HAPPY_MAX_ITERATIONS") {
            if let Ok(i) = iters.parse() {
                config.max_iterations = i;
            }
        }

        // Auto-detect provider from API keys
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if config.api_key.is_empty() {
                config.api_key = key;
                if std::env::var("HAPPY_PROVIDER").is_err() {
                    config.provider = ProviderKind::Anthropic;
                }
            }
        }

        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if config.api_key.is_empty() {
                config.api_key = key;
                if std::env::var("HAPPY_PROVIDER").is_err() {
                    config.provider = ProviderKind::OpenAI;
                }
            }
        }

        // Match API key to provider if both are set
        if config.provider == ProviderKind::Anthropic {
            if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                config.api_key = key;
            }
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            config.api_key = key;
        }

        // Use default model for the provider if still on default
        if config.model == "claude-sonnet-4-20250514" && config.provider == ProviderKind::OpenAI {
            config.model = "gpt-4o".to_string();
        }

        Ok(config)
    }

    /// Interactive setup: prompt user for provider and API key.
    pub fn prompt_setup() -> anyhow::Result<Self> {
        let mut config = Self::default();

        eprintln!("\x1b[1mFirst-time setup\x1b[0m — no API key found.\n");
        eprintln!("Choose a provider:");
        eprintln!("  \x1b[1m1\x1b[0m) Anthropic (Claude)");
        eprintln!("  \x1b[1m2\x1b[0m) OpenAI (GPT)");
        eprintln!("  \x1b[1m3\x1b[0m) OpenAI-compatible (LiteLLM, Ollama, vLLM, etc.)");
        eprint!("\nSelection [1]: ");
        std::io::stderr().flush()?;

        let mut choice = String::new();
        std::io::stdin().read_line(&mut choice)?;
        let choice = choice.trim();

        match choice {
            "2" => {
                config.provider = ProviderKind::OpenAI;
                config.model = "gpt-4o".to_string();
            }
            "3" => {
                config.provider = ProviderKind::OpenAI;
                config.model = "gpt-4o".to_string();

                eprint!("API base URL (e.g. http://localhost:4000): ");
                std::io::stderr().flush()?;
                let mut base = String::new();
                std::io::stdin().read_line(&mut base)?;
                let base = base.trim();
                if !base.is_empty() {
                    config.api_base = Some(base.to_string());
                }

                eprint!("Model name [gpt-4o]: ");
                std::io::stderr().flush()?;
                let mut model = String::new();
                std::io::stdin().read_line(&mut model)?;
                let model = model.trim();
                if !model.is_empty() {
                    config.model = model.to_string();
                }
            }
            // Default: Anthropic
            _ => {}
        }

        eprint!(
            "API key for {}: ",
            match config.provider {
                ProviderKind::Anthropic => "Anthropic",
                ProviderKind::OpenAI => {
                    if config.api_base.is_some() {
                        "your provider"
                    } else {
                        "OpenAI"
                    }
                }
            }
        );
        std::io::stderr().flush()?;

        let mut key = String::new();
        std::io::stdin().read_line(&mut key)?;
        config.api_key = key.trim().to_string();

        if config.api_key.is_empty() {
            anyhow::bail!("No API key provided.");
        }

        Ok(config)
    }

    /// Save config to .happy/agent.toml (excluding the API key from the file,
    /// but setting the environment variable hint).
    pub fn save(&self, repo_path: &str) -> anyhow::Result<()> {
        let dir = Path::new(repo_path).join(".happy");
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }

        // Save provider/model/api_base to TOML, but store the key separately
        let save_config = SaveConfig {
            provider: Some(self.provider),
            model: Some(self.model.clone()),
            api_key: Some(self.api_key.clone()),
            api_base: self.api_base.clone(),
            max_tokens: Some(self.max_tokens),
            temperature: Some(self.temperature),
            max_iterations: Some(self.max_iterations),
        };

        let toml_str = toml::to_string_pretty(&save_config)?;
        let path = dir.join("agent.toml");
        std::fs::write(&path, toml_str)?;
        eprintln!("Saved config to {}", path.display());

        // Remind about .gitignore
        let gitignore = Path::new(repo_path).join(".gitignore");
        if gitignore.exists() {
            let contents = std::fs::read_to_string(&gitignore).unwrap_or_default();
            if !contents.contains(".happy/") && !contents.contains(".happy") {
                eprintln!(
                    "\x1b[33mNote: Add '.happy/' to your .gitignore to keep your API key out of version control.\x1b[0m"
                );
            }
        } else {
            eprintln!(
                "\x1b[33mNote: Add '.happy/' to your .gitignore to keep your API key out of version control.\x1b[0m"
            );
        }

        Ok(())
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Anthropic,
            model: "claude-sonnet-4-20250514".to_string(),
            api_key: String::new(),
            api_base: None,
            max_tokens: 4096,
            temperature: 0.0,
            max_iterations: 20,
        }
    }
}

#[derive(Deserialize)]
struct FileConfig {
    provider: Option<ProviderKind>,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    max_iterations: Option<usize>,
}

#[derive(Serialize)]
struct SaveConfig {
    provider: Option<ProviderKind>,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    max_iterations: Option<usize>,
}
