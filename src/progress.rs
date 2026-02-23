use serde_json::Value;

/// Channel for sending progress notifications during long-running tool calls.
pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<Value>;

/// Unified progress reporting for both MCP and CLI code paths.
#[derive(Clone)]
pub enum ProgressReporter {
    /// CLI output to stderr (respects quiet flag).
    Cli { quiet: bool },
    /// MCP output: eprintln + optional channel sender.
    Mcp { sender: Option<ProgressSender> },
    /// No output.
    Silent,
}

impl ProgressReporter {
    /// Report incremental progress (current/total with message).
    pub fn report(&self, current: usize, total: usize, message: &str) {
        match self {
            Self::Cli { quiet: false } => {
                eprintln!("[{current}/{total}] {message}");
            }
            Self::Mcp { sender } => {
                eprintln!("[{current}/{total}] {message}");
                if let Some(tx) = sender {
                    let _ = tx.send(serde_json::json!({
                        "progress": current,
                        "total": total,
                        "message": message,
                    }));
                }
            }
            Self::Cli { quiet: true } | Self::Silent => {}
        }
    }

    /// Report a phase transition.
    pub fn phase(&self, name: &str) {
        match self {
            Self::Cli { quiet: false } => {
                eprintln!("▸ {name}");
            }
            Self::Mcp { sender } => {
                eprintln!("▸ {name}");
                if let Some(tx) = sender {
                    let _ = tx.send(serde_json::json!({
                        "phase": name,
                    }));
                }
            }
            Self::Cli { quiet: true } | Self::Silent => {}
        }
    }

    /// Emit a general status line.
    pub fn log(&self, message: &str) {
        match self {
            Self::Cli { quiet: false } => {
                eprintln!("{message}");
            }
            Self::Mcp { sender } => {
                eprintln!("{message}");
                if let Some(tx) = sender {
                    let _ = tx.send(serde_json::json!({
                        "message": message,
                    }));
                }
            }
            Self::Cli { quiet: true } | Self::Silent => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_produces_no_output() {
        let p = ProgressReporter::Silent;
        // These should not panic or produce side effects
        p.report(1, 10, "test");
        p.phase("test phase");
        p.log("test log");
    }

    #[test]
    fn cli_quiet_produces_no_output() {
        let p = ProgressReporter::Cli { quiet: true };
        p.report(1, 10, "test");
        p.phase("test phase");
        p.log("test log");
    }

    #[test]
    fn mcp_sends_progress_to_channel() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let p = ProgressReporter::Mcp { sender: Some(tx) };

        p.report(3, 10, "processing");
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg["progress"], 3);
        assert_eq!(msg["total"], 10);
        assert_eq!(msg["message"], "processing");
    }

    #[test]
    fn mcp_sends_phase_to_channel() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let p = ProgressReporter::Mcp { sender: Some(tx) };

        p.phase("Indexing documents");
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg["phase"], "Indexing documents");
    }

    #[test]
    fn mcp_none_sender_does_not_panic() {
        let p = ProgressReporter::Mcp { sender: None };
        p.report(1, 10, "test");
        p.phase("test");
        p.log("test");
    }
}
