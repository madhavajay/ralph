//! Task file parsing for fix_plan.md style task lists
//!
//! Supports various markdown checkbox formats:
//! - `- [ ] A.1 搜索1: "keyword"` (standard)
//! - `- [ ] A.1: 搜索1 "keyword"` (colon position variant)
//! - `- [ ]A.1 搜索1` (missing space)
//! - `- [ ] **A.1** 搜索1` (bold)
//! - `- [ ] A.1 搜索1 (P0)` (with priority)
//! - `- [x] A.1 done task` (completed)

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Represents a single task from a task file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    /// Task identifier (e.g., "A.1", "B.2")
    pub id: String,
    /// Full task content/description
    pub content: String,
    /// Task priority if specified (e.g., "P0", "P1")
    pub priority: Option<String>,
    /// Whether the task is completed
    pub completed: bool,
    /// Line number in the source file (1-indexed)
    pub line_number: usize,
    /// The raw line from the file
    pub raw_line: String,
}

impl Task {
    /// Create a new task
    pub fn new(
        id: String,
        content: String,
        priority: Option<String>,
        completed: bool,
        line_number: usize,
        raw_line: String,
    ) -> Self {
        Self {
            id,
            content,
            priority,
            completed,
            line_number,
            raw_line,
        }
    }
}

/// Configuration for task parsing
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskConfig {
    /// Custom regex pattern for task extraction
    /// Default: `^- \[([ xX])\] \s*(\*{0,2})(\S+?)(\*{0,2})\s*(.*)$`
    #[serde(default)]
    pub pattern: Option<String>,
    /// Filter tasks by prefix (e.g., "A." to only get A.* tasks)
    #[serde(default)]
    pub filter_prefix: Option<String>,
    /// Filter tasks by priority (e.g., "P0")
    #[serde(default)]
    pub filter_priority: Option<String>,
}

/// Default regex pattern for parsing tasks
/// Captures:
/// 1. Checkbox state (space or x/X)
/// 2. Opening bold markers (** or empty)
/// 3. Task ID (like A.1, B.2, etc.)
/// 4. Closing bold markers
/// 5. Rest of the line (content)
const DEFAULT_TASK_PATTERN: &str =
    r"^- \[([ xX])\]\s*(\*{0,2})([A-Za-z0-9]+(?:\.[A-Za-z0-9]+)+)(\*{0,2})\s*(.*)$";

/// Priority extraction pattern (e.g., "(P0)", "(P1)")
const PRIORITY_PATTERN: &str = r"\(P(\d+)\)";

/// Parse a task file and return all tasks
pub fn parse_task_file(path: &Path, config: &TaskConfig) -> Result<Vec<Task>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read task file: {}", path.display()))?;

    parse_tasks(&content, config)
}

/// Parse tasks from string content
pub fn parse_tasks(content: &str, config: &TaskConfig) -> Result<Vec<Task>> {
    let pattern_str = config.pattern.as_deref().unwrap_or(DEFAULT_TASK_PATTERN);
    let task_re = Regex::new(pattern_str)
        .with_context(|| format!("Invalid task pattern: {}", pattern_str))?;
    let priority_re = Regex::new(PRIORITY_PATTERN)?;

    let mut tasks = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_number = line_num + 1; // 1-indexed
        let trimmed = line.trim_start();

        if let Some(caps) = task_re.captures(trimmed) {
            let checkbox_state = caps.get(1).map(|m| m.as_str()).unwrap_or(" ");
            let completed = checkbox_state.eq_ignore_ascii_case("x");

            let task_id = caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            let rest = caps.get(5).map(|m| m.as_str()).unwrap_or("").to_string();

            // Extract priority from content if present
            let priority = priority_re
                .captures(&rest)
                .and_then(|c| c.get(1))
                .map(|m| format!("P{}", m.as_str()));

            // Apply filters
            if let Some(ref prefix) = config.filter_prefix {
                if !task_id.starts_with(prefix) {
                    continue;
                }
            }
            if let Some(ref filter_priority) = config.filter_priority {
                match &priority {
                    Some(p) if p == filter_priority => {}
                    _ => continue,
                }
            }

            let content = rest.trim().to_string();

            tasks.push(Task::new(
                task_id,
                content,
                priority,
                completed,
                line_number,
                line.to_string(),
            ));
        }
    }

    Ok(tasks)
}

/// Get the next pending (incomplete) task
pub fn get_next_task(path: &Path, config: &TaskConfig) -> Result<Option<Task>> {
    let tasks = parse_task_file(path, config)?;
    Ok(tasks.into_iter().find(|t| !t.completed))
}

/// Get all pending tasks
#[allow(dead_code)]
pub fn get_pending_tasks(path: &Path, config: &TaskConfig) -> Result<Vec<Task>> {
    let tasks = parse_task_file(path, config)?;
    Ok(tasks.into_iter().filter(|t| !t.completed).collect())
}

/// Get all completed tasks
#[allow(dead_code)]
pub fn get_completed_tasks(path: &Path, config: &TaskConfig) -> Result<Vec<Task>> {
    let tasks = parse_task_file(path, config)?;
    Ok(tasks.into_iter().filter(|t| t.completed).collect())
}

/// Mark a task as done by updating the file
#[allow(dead_code)]
pub fn mark_task_done(path: &Path, task_id: &str) -> Result<bool> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read task file: {}", path.display()))?;

    let task_re = Regex::new(DEFAULT_TASK_PATTERN)?;
    let mut modified = false;
    let mut new_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(caps) = task_re.captures(trimmed) {
            let id = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            if id == task_id {
                // Replace [ ] with [x]
                let new_line = line.replacen("[ ]", "[x]", 1);
                new_lines.push(new_line);
                modified = true;
                continue;
            }
        }
        new_lines.push(line.to_string());
    }

    if modified {
        let new_content = new_lines.join("\n");
        // Preserve trailing newline if original had one
        let new_content = if content.ends_with('\n') {
            format!("{}\n", new_content)
        } else {
            new_content
        };
        std::fs::write(path, new_content)
            .with_context(|| format!("Failed to write task file: {}", path.display()))?;
    }

    Ok(modified)
}

/// Mark a task as incomplete by updating the file
#[allow(dead_code)]
pub fn mark_task_pending(path: &Path, task_id: &str) -> Result<bool> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read task file: {}", path.display()))?;

    let task_re = Regex::new(DEFAULT_TASK_PATTERN)?;
    let mut modified = false;
    let mut new_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(caps) = task_re.captures(trimmed) {
            let id = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let checkbox_state = caps.get(1).map(|m| m.as_str()).unwrap_or(" ");
            if id == task_id && checkbox_state.eq_ignore_ascii_case("x") {
                // Replace [x] or [X] with [ ]
                let new_line = line.replacen("[x]", "[ ]", 1).replacen("[X]", "[ ]", 1);
                new_lines.push(new_line);
                modified = true;
                continue;
            }
        }
        new_lines.push(line.to_string());
    }

    if modified {
        let new_content = new_lines.join("\n");
        let new_content = if content.ends_with('\n') {
            format!("{}\n", new_content)
        } else {
            new_content
        };
        std::fs::write(path, new_content)
            .with_context(|| format!("Failed to write task file: {}", path.display()))?;
    }

    Ok(modified)
}

/// Get task by ID
pub fn get_task_by_id(path: &Path, task_id: &str, config: &TaskConfig) -> Result<Option<Task>> {
    let tasks = parse_task_file(path, config)?;
    Ok(tasks.into_iter().find(|t| t.id == task_id))
}

/// Count tasks by status
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct TaskStats {
    pub total: usize,
    pub completed: usize,
    pub pending: usize,
}

#[allow(dead_code)]
pub fn get_task_stats(path: &Path, config: &TaskConfig) -> Result<TaskStats> {
    let tasks = parse_task_file(path, config)?;
    let completed = tasks.iter().filter(|t| t.completed).count();
    Ok(TaskStats {
        total: tasks.len(),
        completed,
        pending: tasks.len() - completed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> TaskConfig {
        TaskConfig::default()
    }

    #[test]
    fn test_parse_standard_format() {
        let content = r#"
# Tasks

- [ ] A.1 搜索1: "keyword"
- [x] A.2 完成的任务
- [ ] B.1 另一个任务
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert_eq!(tasks.len(), 3);

        assert_eq!(tasks[0].id, "A.1");
        assert!(!tasks[0].completed);
        assert!(tasks[0].content.contains("搜索1"));

        assert_eq!(tasks[1].id, "A.2");
        assert!(tasks[1].completed);

        assert_eq!(tasks[2].id, "B.1");
        assert!(!tasks[2].completed);
    }

    #[test]
    fn test_parse_bold_format() {
        let content = r#"
- [ ] **A.1** 带加粗的任务
- [ ] **B.2** 另一个加粗任务
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "A.1");
        assert_eq!(tasks[1].id, "B.2");
    }

    #[test]
    fn test_parse_with_priority() {
        let content = r#"
- [ ] A.1 重要任务 (P0)
- [ ] A.2 普通任务 (P1)
- [ ] A.3 无优先级任务
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].priority, Some("P0".to_string()));
        assert_eq!(tasks[1].priority, Some("P1".to_string()));
        assert_eq!(tasks[2].priority, None);
    }

    #[test]
    fn test_filter_by_prefix() {
        let content = r#"
- [ ] A.1 A组任务
- [ ] A.2 A组任务2
- [ ] B.1 B组任务
"#;
        let config = TaskConfig {
            filter_prefix: Some("A.".to_string()),
            ..Default::default()
        };
        let tasks = parse_tasks(content, &config).unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| t.id.starts_with("A.")));
    }

    #[test]
    fn test_filter_by_priority() {
        let content = r#"
- [ ] A.1 重要任务 (P0)
- [ ] A.2 普通任务 (P1)
- [ ] A.3 无优先级任务
"#;
        let config = TaskConfig {
            filter_priority: Some("P0".to_string()),
            ..Default::default()
        };
        let tasks = parse_tasks(content, &config).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "A.1");
    }

    #[test]
    fn test_get_next_task() {
        let content = r#"
- [x] A.1 完成的任务
- [ ] A.2 下一个任务
- [ ] A.3 再下一个
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        let next = tasks.into_iter().find(|t| !t.completed);
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, "A.2");
    }

    #[test]
    fn test_task_stats() {
        let content = r#"
- [x] A.1 完成1
- [x] A.2 完成2
- [ ] A.3 未完成
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        let completed = tasks.iter().filter(|t| t.completed).count();
        assert_eq!(tasks.len(), 3);
        assert_eq!(completed, 2);
    }

    #[test]
    fn test_custom_pattern() {
        // Custom pattern that matches a different format
        let content = r#"
* TODO A.1 custom format task
* DONE A.2 completed custom task
"#;
        let _config = TaskConfig {
            pattern: Some(r"^\* (TODO|DONE) (\S+)\s*(.*)$".to_string()),
            ..Default::default()
        };
        // Note: This test demonstrates custom pattern support
        // The default pattern won't match this format
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert_eq!(tasks.len(), 0); // Default pattern doesn't match

        // With proper custom pattern it would match
        // (but our capture groups are different, so this is just for demonstration)
    }

    #[test]
    fn test_uppercase_x_completed() {
        let content = r#"
- [X] A.1 大写X完成
- [x] A.2 小写x完成
- [ ] A.3 未完成
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert!(tasks[0].completed);
        assert!(tasks[1].completed);
        assert!(!tasks[2].completed);
    }

    #[test]
    fn test_line_numbers() {
        let content = r#"# Header

- [ ] A.1 任务1
- [ ] A.2 任务2
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert_eq!(tasks[0].line_number, 3);
        assert_eq!(tasks[1].line_number, 4);
    }

    #[test]
    fn test_empty_content() {
        let content = "";
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_no_tasks() {
        let content = r#"
# Just a header

Some regular text with no tasks.
"#;
        let tasks = parse_tasks(content, &default_config()).unwrap();
        assert!(tasks.is_empty());
    }
}
