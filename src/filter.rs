use crate::parser::LogEntry;
use regex::Regex;

#[derive(Clone, Debug)]
pub struct FilterRule {
    pub field: FilterField,
    pub mode: FilterMode,
    pub action: FilterAction,
    pub terms: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilterField {
    Level,
    Timestamp,
    FileLine,
    Message,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilterMode {
    Contains,
    Regex,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilterAction {
    Include,
    Exclude,
}

impl FilterRule {
    pub fn matches(&self, entry: &LogEntry) -> bool {
        let field_value = match self.field {
            FilterField::Level => &entry.level,
            FilterField::Timestamp => &entry.timestamp,
            FilterField::FileLine => &entry.file_line,
            FilterField::Message => &entry.message,
        };

        if self.terms.is_empty() {
            return true;
        }

        self.terms.iter().all(|term| match self.mode {
            FilterMode::Contains => field_value.contains(term),
            FilterMode::Regex => Regex::new(term)
                .map(|re| re.is_match(field_value))
                .unwrap_or(false),
        })
    }

    pub fn from_ui(field: &str, operator: &str, value: &str) -> Option<Self> {
        let field = match field {
            "Level" => FilterField::Level,
            "Timestamp" => FilterField::Timestamp,
            "FileLine" => FilterField::FileLine,
            "Message" => FilterField::Message,
            _ => return None,
        };

        let (action, mode) = match operator {
            "IncludeContains" => (FilterAction::Include, FilterMode::Contains),
            "ExcludeContains" => (FilterAction::Exclude, FilterMode::Contains),
            "IncludeRegex" => (FilterAction::Include, FilterMode::Regex),
            "ExcludeRegex" => (FilterAction::Exclude, FilterMode::Regex),
            _ => return None,
        };

        let terms: Vec<String> = std::iter::once(value.trim())
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        if terms.is_empty() {
            return None;
        }

        Some(Self {
            field,
            mode,
            action,
            terms,
        })
    }
}

pub struct FilterManager {
    pub rules: Vec<FilterRule>,
}

impl FilterManager {
    pub fn new() -> Self {
        Self { rules: vec![] }
    }

    pub fn add_rule(&mut self, rule: FilterRule) {
        self.rules.push(rule);
    }

    pub fn remove_rule(&mut self, index: usize) {
        if index < self.rules.len() {
            self.rules.remove(index);
        }
    }

    pub fn matches(&self, entry: &LogEntry) -> bool {
        if self.rules.is_empty() {
            return true;
        }

        for rule in &self.rules {
            let matched = rule.matches(entry);
            match rule.action {
                FilterAction::Include if !matched => return false,
                FilterAction::Exclude if matched => return false,
                _ => {}
            }
        }

        true
    }
}
