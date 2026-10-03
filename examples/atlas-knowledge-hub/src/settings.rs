use std::path::PathBuf;

/// Runtime settings, read from the environment.
#[derive(Debug, Clone)]
pub struct Settings {
    pub port: u16,
    pub mcp_port: u16,
    pub ollama_url: String,
    pub chat_model: String,
    pub enrich_model: String,
    pub anthropic_key: Option<String>,
    pub auth_secret: String,
    pub data_dir: PathBuf,
    pub keep_data: bool,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_port(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl Settings {
    pub fn from_env() -> Settings {
        Settings {
            port: env_port("PORT", 8080),
            mcp_port: env_port("MCP_PORT", 8081),
            ollama_url: env_or("OLLAMA_URL", "http://localhost:11434"),
            chat_model: env_or("ATLAS_CHAT_MODEL", "qwen2.5"),
            enrich_model: env_or("ATLAS_ENRICH_MODEL", "qwen2.5"),
            anthropic_key: std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .filter(|v| !v.is_empty()),
            auth_secret: env_or(
                "ARCANUM_AUTH_SECRET",
                "arcanum-dev-secret-minimum-32chars!!",
            ),
            data_dir: PathBuf::from("data"),
            keep_data: std::env::var("ATLAS_KEEP_DATA")
                .map(|v| !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"))
                .unwrap_or(false),
        }
    }

    /// Fixed values independent of the environment, for tests.
    pub fn for_tests(dir: PathBuf) -> Settings {
        Settings {
            port: 8080,
            mcp_port: 8081,
            ollama_url: "http://localhost:11434".into(),
            chat_model: "qwen2.5".into(),
            enrich_model: "qwen2.5".into(),
            anthropic_key: None,
            auth_secret: "arcanum-dev-secret-minimum-32chars!!".into(),
            data_dir: dir,
            keep_data: false,
        }
    }
}
