/// What to do with a matched event.
#[derive(Debug, Clone)]
pub enum DispatchAction {
    /// Build prompt, call LLM
    AgentTurn,
    /// Direct handler, no LLM
    Handler(String),
    /// Ignore the event
    Drop,
}

/// A dispatch rule: pattern → action.
#[derive(Debug, Clone)]
pub struct DispatchRule {
    pub pattern: String,
    pub action: DispatchAction,
    pub prompt_config: Option<String>,
}

/// Routes events to actions based on dispatch key matching.
pub struct Dispatcher {
    rules: Vec<DispatchRule>,
}

impl Dispatcher {
    /// Create a dispatcher that sends everything to AgentTurn.
    pub fn default_agent_turn() -> Self {
        Self {
            rules: vec![DispatchRule {
                pattern: "*".into(),
                action: DispatchAction::AgentTurn,
                prompt_config: None,
            }],
        }
    }

    /// Match a dispatch key against rules. Returns the first matching rule.
    /// For now, always returns the default (first) rule — no glob matching yet.
    pub fn match_rule(&self, _key: &str) -> &DispatchRule {
        // TODO: implement glob pattern matching
        &self.rules[0]
    }
}
