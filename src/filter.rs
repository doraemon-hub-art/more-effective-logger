use crate::parser::LogEntry;

#[derive(Clone, Debug)]
pub struct FilterRule {
    pub field: FilterField,
    pub operator: FilterOperator,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum FilterField {
    Level,
    Timestamp,
    FileLine,
    Message,
}

impl std::fmt::Display for FilterField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterField::Level => write!(f, "Level"),
            FilterField::Timestamp => write!(f, "Timestamp"),
            FilterField::FileLine => write!(f, "FileLine"),
            FilterField::Message => write!(f, "Message"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum FilterOperator {
    Contains,
    NotContains,
    Equals,
    NotEquals,
}

impl std::fmt::Display for FilterOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterOperator::Contains => write!(f, "Contains"),
            FilterOperator::NotContains => write!(f, "NotContains"),
            FilterOperator::Equals => write!(f, "Equals"),
            FilterOperator::NotEquals => write!(f, "NotEquals"),
        }
    }
}

impl FilterRule {
    pub fn matches(&self, entry: &LogEntry) -> bool {
        let field_value = match self.field {
            FilterField::Level => &entry.level,
            FilterField::Timestamp => &entry.timestamp,
            FilterField::FileLine => &entry.file_line,
            FilterField::Message => &entry.message,
        };

        match self.operator {
            FilterOperator::Contains => field_value.contains(&self.value),
            FilterOperator::NotContains => !field_value.contains(&self.value),
            FilterOperator::Equals => field_value == &self.value,
            FilterOperator::NotEquals => field_value != &self.value,
        }
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
        // 任意一条规则匹配即可（OR 逻辑）
        self.rules.iter().any(|rule| rule.matches(entry))
    }
}
