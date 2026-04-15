use regex::Regex;

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub level: String,
    pub timestamp: String,
    pub file_line: String,
    pub message: String,
}

pub fn parse_log_line(line: &str) -> Option<LogEntry> {
    // 匹配格式: [level][timestamp] [file:line] message
    // 例如: I2026/04/13 07:23:50 545166 [aws_kvs_common.hpp:771]
    
    let re = Regex::new(r"^([A-Z])(\d{4}/\d{2}/\d{2}\s+\d{2}:\d{2}:\d{2}\s+\d+)\s+\[([^\]]+)\]\s+(.*)$").ok()?;
    
    if let Some(caps) = re.captures(line) {
        Some(LogEntry {
            level: caps.get(1)?.as_str().to_string(),
            timestamp: caps.get(2)?.as_str().to_string(),
            file_line: caps.get(3)?.as_str().to_string(),
            message: caps.get(4)?.as_str().to_string(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log() {
        let line = "I2026/04/13 07:23:50 545166 [aws_kvs_common.hpp:771] [INFO] : MTU/Send Discarded Packets: 0";
        let entry = parse_log_line(line);
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.timestamp, "I2026/04/13 07:23:50 545166");
        assert_eq!(e.file_line, "aws_kvs_common.hpp:771");
    }
}
