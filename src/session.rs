use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub host: String,
    pub username: String,
    pub password: String,
}

impl Session {
    pub fn new(host: String, username: String, password: String) -> Self {
        Self {
            host,
            username,
            password,
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        let parts: Vec<&str> = value.split('|').collect();
        if parts.len() != 3 {
            return None;
        }

        Some(Self {
            host: parts[0].to_string(),
            username: parts[1].to_string(),
            password: parts[2].to_string(),
        })
    }

    pub fn to_wire(&self) -> String {
        format!("{}|{}|{}", self.host, self.username, self.password)
    }
}
