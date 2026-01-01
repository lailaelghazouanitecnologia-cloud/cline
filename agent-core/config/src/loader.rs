use agent_common::{AgentError, AgentResult};
use std::path::{Path, PathBuf};

use crate::config::{Config, ConfigOverrides};

const CONFIG_FILE_NAME: &str = "config.toml";

pub struct ConfigLoader {
    agent_home: PathBuf,
}

impl ConfigLoader {
    pub fn new(agent_home: PathBuf) -> Self {
        Self { agent_home }
    }

    pub fn from_default_home() -> AgentResult<Self> {
        let home = find_agent_home()?;
        Ok(Self::new(home))
    }

    pub fn load(&self) -> AgentResult<Config> {
        let config_path = self.agent_home.join(CONFIG_FILE_NAME);

        if !config_path.exists() {
            return Ok(Config::default());
        }

        self.load_from_path(&config_path)
    }

    pub fn load_with_overrides(&self, overrides: ConfigOverrides) -> AgentResult<Config> {
        let mut config = self.load()?;
        overrides.apply(&mut config);
        Ok(config)
    }

    fn load_from_path(&self, path: &Path) -> AgentResult<Config> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| AgentError::configuration(e.to_string()))
    }

    pub fn save(&self, config: &Config) -> AgentResult<()> {
        std::fs::create_dir_all(&self.agent_home)?;

        let config_path = self.agent_home.join(CONFIG_FILE_NAME);
        let content =
            toml::to_string_pretty(config).map_err(|e| AgentError::serialization(e.to_string()))?;

        std::fs::write(config_path, content)?;
        Ok(())
    }
}

fn find_agent_home() -> AgentResult<PathBuf> {
    if let Ok(home) = std::env::var("CLINE_HOME") {
        return Ok(PathBuf::from(home));
    }

    dirs::home_dir()
        .map(|home| home.join(".cline"))
        .ok_or_else(|| AgentError::configuration("cannot determine home directory"))
}
