use crate::session::Session;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub session: Option<Session>,
}

impl Default for Config {
    fn default() -> Self {
        Self { session: None }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/melogger");
        
        config_dir.join("user_config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    match toml::from_str(&content) {
                        Ok(config) => config,
                        Err(e) => {
                            eprintln!("Failed to parse config: {}", e);
                            Config::default()
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read config: {}", e);
                    Config::default()
                }
            }
        } else {
            Config::default()
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        
        // 创建目录
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        
        Ok(())
    }

    pub fn save_session(&mut self, session: Session) -> Result<(), Box<dyn std::error::Error>> {
        self.session = Some(session);
        self.save()
    }

    pub fn get_session(&self) -> Option<&Session> {
        self.session.as_ref()
    }
}
