use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 应用配置根结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 窗口设置
    #[serde(default)]
    pub window: WindowConfig,

    /// 字体设置
    #[serde(default)]
    pub font: FontConfig,

    /// 主题
    #[serde(default)]
    pub theme: ThemeConfig,

    /// 默认布局
    #[serde(default)]
    pub layout: LayoutConfig,

    /// SSH 会话（历史兼容）
    #[serde(default)]
    pub session: Option<SessionConfig>,
}

/// 窗口配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_width")]
    pub width: f64,
    #[serde(default = "default_height")]
    pub height: f64,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

/// 字体配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    #[serde(default = "default_font_family")]
    pub family: String,
    #[serde(default = "default_font_size")]
    pub size: u32,
}

/// 主题配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub cursor: Option<String>,
    pub black: Option<String>,
    pub red: Option<String>,
    pub green: Option<String>,
    pub yellow: Option<String>,
    pub blue: Option<String>,
    pub magenta: Option<String>,
    pub cyan: Option<String>,
    pub white: Option<String>,
    pub bright_black: Option<String>,
    pub bright_red: Option<String>,
    pub bright_green: Option<String>,
    pub bright_yellow: Option<String>,
    pub bright_blue: Option<String>,
    pub bright_magenta: Option<String>,
    pub bright_cyan: Option<String>,
    pub bright_white: Option<String>,
    pub selection_background: Option<String>,
    pub cursor_accent: Option<String>,
}

/// 默认布局
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// 布局类型：single_terminal | horizontal_split
    #[serde(default = "default_layout_type")]
    pub default_type: String,
}

/// SSH 会话配置（兼容旧代码）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub host: String,
    pub username: String,
    pub password: String,
}

// ── 默认值 ──────────────────────────────────────

fn default_width() -> f64 { 1200.0 }
fn default_height() -> f64 { 800.0 }
fn default_opacity() -> f64 { 0.95 }
fn default_font_family() -> String { "JetBrains Mono".into() }
fn default_font_size() -> u32 { 13 }
fn default_layout_type() -> String { "single_terminal".into() }

impl Default for WindowConfig {
    fn default() -> Self {
        Self { width: default_width(), height: default_height(), opacity: default_opacity() }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self { family: default_font_family(), size: default_font_size() }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: None, foreground: None, cursor: None,
            black: None, red: None, green: None, yellow: None,
            blue: None, magenta: None, cyan: None, white: None,
            bright_black: None, bright_red: None, bright_green: None,
            bright_yellow: None, bright_blue: None, bright_magenta: None,
            bright_cyan: None, bright_white: None,
            selection_background: None, cursor_accent: None,
        }
    }
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self { default_type: default_layout_type() }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            font: FontConfig::default(),
            theme: ThemeConfig::default(),
            layout: LayoutConfig::default(),
            session: None,
        }
    }
}

// ── 加载/保存 ────────────────────────────────────

impl AppConfig {
    /// 配置文件路径: ~/.config/melogger/config.toml
    pub fn config_path() -> PathBuf {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("melogger");
        dir.join("config.toml")
    }

    /// 加载配置，文件不存在则返回默认值
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(cfg) => {
                        log::info!("Config loaded from {}", path.display());
                        cfg
                    }
                    Err(e) => {
                        log::warn!("Config parse error: {e}, using defaults");
                        Self::default()
                    }
                },
                Err(e) => {
                    log::warn!("Config read error: {e}, using defaults");
                    Self::default()
                }
            }
        } else {
            log::info!("No config file found at {}, using defaults", path.display());
            Self::default()
        }
    }

    /// 写入配置文件（如果不存在则创建）
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        log::info!("Config saved to {}", path.display());
        Ok(())
    }
}
