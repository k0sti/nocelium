use serde::{Deserialize, Serialize};

/// What to do with a matched event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DispatchAction {
    /// Build prompt, call LLM
    AgentTurn,
    /// Direct handler, no LLM
    Handler {
        name: String,
    },
    /// Ignore the event
    Drop,
}

/// A dispatch rule: pattern → action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchRule {
    pub pattern: String,
    pub action: DispatchAction,
    #[serde(default)]
    pub prompt_config: Option<String>,
}

/// Routes events to actions based on dispatch key matching.
pub struct Dispatcher {
    rules: Vec<DispatchRule>,
}

impl Dispatcher {
    /// Create a dispatcher with custom rules + a catch-all AgentTurn fallback.
    pub fn new(mut rules: Vec<DispatchRule>) -> Self {
        // Ensure there's always a catch-all
        let has_catchall = rules.iter().any(|r| r.pattern == "*");
        if !has_catchall {
            rules.push(DispatchRule {
                pattern: "*".into(),
                action: DispatchAction::AgentTurn,
                prompt_config: None,
            });
        }
        Self { rules }
    }

    /// Create a dispatcher that sends everything to AgentTurn.
    pub fn default_agent_turn() -> Self {
        Self::new(vec![])
    }

    /// Match a dispatch key against rules. Returns the first matching rule.
    pub fn match_rule(&self, key: &str) -> &DispatchRule {
        for rule in &self.rules {
            if glob_match(&rule.pattern, key) {
                return rule;
            }
        }
        // Should never reach here due to catch-all, but be safe
        self.rules.last().unwrap()
    }
}

/// Simple glob matching supporting `*` (any segment) and `**` / bare `*` (match all).
///
/// Patterns are colon-separated segments matched against colon-separated key segments.
/// - `*` matches exactly one segment
/// - `**` or a trailing `*` matches one or more remaining segments
/// - Literal segments must match exactly
///
/// Examples:
///   `telegram:message:*` matches `telegram:message:-1001234`
///   `telegram:message:*` does NOT match `telegram:message:-1001234:42`
///   `telegram:*` matches `telegram:message` but not `telegram:message:123`
///   `*` matches everything
fn glob_match(pattern: &str, key: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let pat_parts: Vec<&str> = pattern.split(':').collect();
    let key_parts: Vec<&str> = key.split(':').collect();

    let mut pi = 0;
    let mut ki = 0;

    while pi < pat_parts.len() && ki < key_parts.len() {
        match pat_parts[pi] {
            "**" => return true, // matches everything remaining
            "*" => {
                // If this is the last pattern segment, it must match exactly the last key segment
                if pi == pat_parts.len() - 1 {
                    return ki == key_parts.len() - 1;
                }
                pi += 1;
                ki += 1;
            }
            literal => {
                if literal != key_parts[ki] {
                    return false;
                }
                pi += 1;
                ki += 1;
            }
        }
    }

    pi == pat_parts.len() && ki == key_parts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("telegram:message:-1001234", "telegram:message:-1001234"));
        assert!(!glob_match("telegram:message:-1001234", "telegram:message:-9999"));
    }

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("telegram:message:*", "telegram:message:-1001234"));
        assert!(!glob_match("telegram:message:*", "telegram:message:-1001234:42"));
    }

    #[test]
    fn test_glob_match_star_middle() {
        assert!(glob_match("telegram:*:direct", "telegram:message:direct"));
        assert!(!glob_match("telegram:*:direct", "telegram:message:group"));
    }

    #[test]
    fn test_glob_match_doublestar() {
        assert!(glob_match("telegram:**", "telegram:message:-1001234:42"));
        assert!(glob_match("telegram:**", "telegram:callback:approve"));
    }

    #[test]
    fn test_glob_match_catchall() {
        assert!(glob_match("*", "telegram:message:-1001234"));
        assert!(glob_match("*", "cron:heartbeat"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn test_glob_no_match_different_length() {
        assert!(!glob_match("telegram:message", "telegram:message:-1001234"));
        assert!(!glob_match("telegram:message:*:*", "telegram:message:-1001234"));
    }

    #[test]
    fn test_dispatcher_first_match_wins() {
        let d = Dispatcher::new(vec![
            DispatchRule {
                pattern: "telegram:message:direct:*".into(),
                action: DispatchAction::Handler { name: "dm_handler".into() },
                prompt_config: None,
            },
            DispatchRule {
                pattern: "telegram:message:*".into(),
                action: DispatchAction::AgentTurn,
                prompt_config: None,
            },
        ]);

        match &d.match_rule("telegram:message:direct:60996061").action {
            DispatchAction::Handler { name } => assert_eq!(name, "dm_handler"),
            _ => panic!("Expected handler"),
        }

        match &d.match_rule("telegram:message:-1001234").action {
            DispatchAction::AgentTurn => {}
            _ => panic!("Expected agent turn"),
        }
    }
}
