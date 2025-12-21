//! Interactive mode with TUI.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, LocalSet};
use tokio::time;

use revibe_core::events::AgentEvent;
use revibe_core::modes::AgentMode;
use revibe_core::types::ApprovalResponse;
use revibe_core::{Agent, VibeConfig};

use crate::args::Args;
use crate::ui::messages::SystemMessageKind;
use crate::ui::{App, AppAction};

/// Approval request sent from agent to UI.
#[derive(Debug)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub args: serde_json::Value,
    pub tool_call_id: String,
    pub response_tx: oneshot::Sender<(ApprovalResponse, Option<String>)>,
}

/// Result type for agent task completion.
type AgentTaskResult = Result<(), String>;

/// Control flow for the main loop.
enum LoopControl {
    Continue,
    Break,
}

/// Holds state for the current agent task.
struct AgentTaskState {
    /// Whether the agent is running.
    running: bool,
    /// Handle to the spawned task (for cancellation).
    handle: Option<JoinHandle<()>>,
}

impl AgentTaskState {
    fn new() -> Self {
        Self {
            running: false,
            handle: None,
        }
    }

    fn start(&mut self, handle: JoinHandle<()>) {
        self.running = true;
        self.handle = Some(handle);
    }

    fn stop(&mut self) {
        self.running = false;
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }

    fn complete(&mut self) {
        self.running = false;
        self.handle = None;
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

type SharedAgent = Rc<RefCell<Option<Agent>>>;

fn with_agent_ref<R, F>(agent: &SharedAgent, op: F) -> Result<R>
where
    F: FnOnce(&Agent) -> R,
{
    let borrow = agent.borrow();
    let agent_ref = borrow
        .as_ref()
        .ok_or_else(|| anyhow!("Agent is unavailable"))?;
    Ok(op(agent_ref))
}

fn with_agent_mut<R, F>(agent: &SharedAgent, op: F) -> Result<R>
where
    F: FnOnce(&mut Agent) -> R,
{
    let mut slot = agent.borrow_mut();
    let mut agent_value = slot.take().ok_or_else(|| anyhow!("Agent is unavailable"))?;
    let result = op(&mut agent_value);
    *slot = Some(agent_value);
    Ok(result)
}

async fn with_agent_async<R, F, Fut>(agent: &SharedAgent, op: F) -> Result<R>
where
    F: FnOnce(Agent) -> Fut,
    Fut: std::future::Future<Output = (Agent, R)>,
{
    let agent_value = {
        let mut slot = agent.borrow_mut();
        slot.take().ok_or_else(|| anyhow!("Agent is unavailable"))?
    };

    let (agent_value, result) = op(agent_value).await;

    let mut slot = agent.borrow_mut();
    *slot = Some(agent_value);

    Ok(result)
}

/// Run interactive mode.
pub async fn run(
    config: VibeConfig,
    mode: AgentMode,
    initial_prompt: Option<String>,
    args: &Args,
    loaded_messages: Option<Vec<revibe_llm::LlmMessage>>,
) -> Result<()> {
    // Handle enabled_tools override
    let mut config = config.clone();
    if !args.enabled_tools.is_empty() {
        config.enabled_tools = args.enabled_tools.clone();
        config.disabled_tools.clear();
    }

    // Create the TUI app
    let mut app = App::new(config.clone(), mode);

    // Initialize version update checker in async context
    if config.enable_update_checks {
        let cache_dir = revibe_core::paths::CONFIG_FILE.parent().unwrap();
        app.version_update_checker =
            Some(crate::ui::version_update::VersionUpdateChecker::new(cache_dir).await);
    }

    // Set compact threshold for token display
    if config.auto_compact_threshold > 0 {
        app.update_tokens(0, config.auto_compact_threshold);
    }

    // Check for version updates
    if let Err(e) = app.check_version_updates().await {
        tracing::warn!("Failed to check for version updates: {}", e);
    }

    // Show dangerous directory warning
    app.show_dangerous_directory_warning();

    // Create terminal wrapper
    let mut terminal = match crate::ui::app::TerminalWrapper::new() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to initialize terminal: {e}");
            return Err(e.into());
        }
    };

    // Create agent wrapped in Rc<RefCell> for local sharing
    let agent: SharedAgent = Rc::new(RefCell::new(Some(
        Agent::with_options(config.clone(), mode, None, None, true).await?,
    )));

    // Load previous messages if provided
    if let Some(messages) = loaded_messages {
        let history_result = with_agent_mut(&agent, |agent| agent.load_history(messages))?;
        history_result?;
        app.add_system_message(
            "Loaded previous session".to_string(),
            SystemMessageKind::Info,
        );
    }

    // Store session ID for resume message
    let session_id = with_agent_ref(&agent, |agent_ref| agent_ref.session_id().to_string())?;
    app.set_session_id(session_id);

    // Use a LocalSet to run non-Send futures on the current thread
    let local = LocalSet::new();

    // Channel for receiving agent events
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(100);
    // Channel for signaling agent completion
    let (done_tx, mut done_rx) = mpsc::channel::<AgentTaskResult>(1);
    // Channel for approval requests
    let (approval_tx, mut approval_rx) = mpsc::channel::<ApprovalRequest>(10);

    // Set up approval callback for non-auto-approve mode
    if !mode.auto_approve() {
        let approval_tx_clone = approval_tx.clone();
        with_agent_mut(&agent, |agent| {
            agent.set_approval_callback(Arc::new(move |tool_name, args, tool_call_id| {
                let approval_tx = approval_tx_clone.clone();
                Box::pin(async move {
                    let (response_tx, response_rx) = oneshot::channel();
                    let request = ApprovalRequest {
                        tool_name,
                        args,
                        tool_call_id,
                        response_tx,
                    };

                    // Send approval request to UI
                    if approval_tx.try_send(request).is_err() {
                        // Channel busy, deny by default
                        return (
                            ApprovalResponse::No,
                            Some("Approval channel busy".to_string()),
                        );
                    }

                    // Wait for response from UI with timeout to prevent freezing
                    use tokio::time::{Duration, timeout};
                    match timeout(Duration::from_secs(30), response_rx).await {
                        Ok(Ok(response)) => response,
                        Ok(Err(_)) => {
                            (ApprovalResponse::No, Some("Approval cancelled".to_string()))
                        }
                        Err(_) => (
                            ApprovalResponse::No,
                            Some("Approval timeout - no response received".to_string()),
                        ),
                    }
                })
            }));
        })?;
    }

    // Track agent task state (with cancellation support)
    let agent_task = Rc::new(RefCell::new(AgentTaskState::new()));

    // Run the main loop within the LocalSet
    local
        .run_until(async {
            // Pending approval response sender
            let mut pending_approval: Option<oneshot::Sender<(ApprovalResponse, Option<String>)>> =
                None;

            // Process initial prompt if provided
            if let Some(prompt) = initial_prompt {
                app.add_user_message(prompt.clone(), true);
                app.start_loading();

                let agent_clone = Rc::clone(&agent);
                let event_tx_clone = event_tx.clone();
                let done_tx_clone = done_tx.clone();

                let handle = tokio::task::spawn_local(async move {
                    run_agent_task(agent_clone, prompt, event_tx_clone, done_tx_clone).await;
                });
                agent_task.borrow_mut().start(handle);
            }

            loop {
                // Tick animations
                app.tick();

                // Draw
                if terminal.draw(&mut app).is_err() {
                    break;
                }

                // IMPORTANT: Process terminal events FIRST before waiting on agent events.
                // This ensures scrolling and other UI interactions are responsive during agent execution.
                // (Matching Mistral Vibe's Textual behavior where key events are always processed promptly)
                while let Ok(true) = event::poll(Duration::from_millis(0)) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            let action = app.handle_key(key);

                            match handle_app_action(
                                action,
                                &mut app,
                                &agent,
                                &agent_task,
                                &event_tx,
                                &done_tx,
                                &mut pending_approval,
                            )
                            .await
                            {
                                LoopControl::Continue => {}
                                LoopControl::Break => {
                                    // Restore terminal before breaking
                                    drop(terminal);
                                    if let Some(session_id) = &app.session_id {
                                        println!();
                                        println!(
                                            "To continue this session, run: revibe --continue"
                                        );
                                        println!(
                                            "Or: revibe --resume {}",
                                            &session_id[..8.min(session_id.len())]
                                        );
                                    }
                                    return;
                                }
                            }
                        }
                        Ok(Event::Mouse(mouse)) => {
                            // Handle mouse scroll events for chat scrolling
                            app.handle_mouse(mouse);
                        }
                        _ => {}
                    }
                }

                // Process ALL available agent events before waiting
                // This prevents events from being dropped when they come in rapidly
                loop {
                    // First, try to receive approval requests (highest priority)
                    match approval_rx.try_recv() {
                        Ok(request) => {
                            app.show_approval(
                                request.tool_name.clone(),
                                request.args.clone(),
                                request.tool_call_id.clone(),
                            );
                            pending_approval = Some(request.response_tx);
                            continue;
                        }
                        Err(mpsc::error::TryRecvError::Empty) => {}
                        Err(mpsc::error::TryRecvError::Disconnected) => {}
                    }

                    // Process all available agent events
                    match event_rx.try_recv() {
                        Ok(event) => {
                            handle_agent_event(&mut app, event);
                            continue;
                        }
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => break,
                    }
                }

                // Check for agent completion (non-blocking)
                match done_rx.try_recv() {
                    Ok(result) => {
                        agent_task.borrow_mut().complete();
                        app.stop_loading();
                        app.complete_assistant_message();

                        // Update token context from agent stats
                        if let Ok(context_tokens) =
                            with_agent_ref(&agent, |agent_ref| agent_ref.stats().context_tokens)
                        {
                            app.update_tokens(context_tokens, app.token_context.1);
                        }

                        if let Err(e) = result {
                            app.add_system_message(format!("Error: {e}"), SystemMessageKind::Error);
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {}
                    Err(mpsc::error::TryRecvError::Disconnected) => {}
                }

                // Short sleep for animation updates - only if no events were processed
                time::sleep(Duration::from_millis(16)).await; // ~60 FPS
            }

            // Restore terminal
            drop(terminal);

            // Print session resume message
            if let Some(session_id) = &app.session_id {
                println!();
                println!("To continue this session, run: revibe --continue");
                println!(
                    "Or: revibe --resume {}",
                    &session_id[..8.min(session_id.len())]
                );
            }
        })
        .await;

    Ok(())
}

/// Run the agent task and send events directly to the provided channel.
///
/// The agent's `act()` method now takes a sender directly, so events are
/// sent in real-time as the conversation progresses. This function runs
/// the agent to completion and then signals via done_tx.
async fn run_agent_task(
    agent: SharedAgent,
    prompt: String,
    event_tx: mpsc::Sender<AgentEvent>,
    done_tx: mpsc::Sender<AgentTaskResult>,
) {
    // Run the agent - events are sent directly to event_tx as they happen
    let result = with_agent_async(&agent, |mut agent| async move {
        let result = agent.act(&prompt, event_tx).await;
        (agent, result)
    })
    .await;

    // Signal completion
    match result {
        Ok(Ok(())) => {
            let _ = done_tx.send(Ok(())).await;
        }
        Ok(Err(e)) => {
            let _ = done_tx.send(Err(e.to_string())).await;
        }
        Err(e) => {
            let _ = done_tx.send(Err(e.to_string())).await;
        }
    }
}

/// Handle an agent event.
fn handle_agent_event(app: &mut App, event: AgentEvent) {
    match event {
        AgentEvent::Assistant(e) => {
            if !e.content.is_empty() {
                app.update_assistant_message(&e.content);
            }
        }
        AgentEvent::ToolCall(e) => {
            app.add_tool_call(e.tool_name, &e.args);
        }
        AgentEvent::ToolResult(e) => {
            app.add_tool_result(&e);
        }
        AgentEvent::CompactStart(e) => {
            app.add_system_message(
                format!(
                    "Compacting conversation ({}k tokens > {}k threshold)...",
                    e.current_context_tokens / 1000,
                    e.threshold / 1000
                ),
                SystemMessageKind::Info,
            );
        }
        AgentEvent::CompactEnd(e) => {
            app.add_system_message(
                format!(
                    "Compacted: {}k → {}k tokens",
                    e.old_context_tokens / 1000,
                    e.new_context_tokens / 1000
                ),
                SystemMessageKind::Info,
            );
            app.update_tokens(e.new_context_tokens, app.token_context.1);
        }
    }
}

/// Handle an app action from user input.
async fn handle_app_action(
    action: AppAction,
    app: &mut App,
    agent: &SharedAgent,
    agent_task: &Rc<RefCell<AgentTaskState>>,
    event_tx: &mpsc::Sender<AgentEvent>,
    done_tx: &mpsc::Sender<AgentTaskResult>,
    pending_approval: &mut Option<oneshot::Sender<(ApprovalResponse, Option<String>)>>,
) -> LoopControl {
    match action {
        AppAction::Continue => LoopControl::Continue,
        AppAction::Exit => {
            // Cancel any running task before exiting
            agent_task.borrow_mut().stop();
            app.should_exit = true;
            LoopControl::Break
        }
        AppAction::Submit(content) => {
            // Only accept new input if not already processing
            if !agent_task.borrow().is_running() {
                app.add_user_message(content.clone(), false);
                app.start_loading();

                let agent_clone = Rc::clone(agent);
                let event_tx_clone = event_tx.clone();
                let done_tx_clone = done_tx.clone();

                let handle = tokio::task::spawn_local(async move {
                    run_agent_task(agent_clone, content, event_tx_clone, done_tx_clone).await;
                });
                agent_task.borrow_mut().start(handle);
            }
            LoopControl::Continue
        }
        AppAction::Command(cmd) => {
            // Only process commands if not already processing
            if !agent_task.borrow().is_running() {
                let mut agent_value = {
                    let mut slot = agent.borrow_mut();
                    match slot.take() {
                        Some(agent_inner) => agent_inner,
                        None => {
                            app.add_system_message(
                                "Agent unavailable".to_string(),
                                SystemMessageKind::Error,
                            );
                            return LoopControl::Continue;
                        }
                    }
                };

                let command_result = handle_command(app, &mut agent_value, &cmd).await;

                {
                    let mut slot = agent.borrow_mut();
                    *slot = Some(agent_value);
                }

                match command_result {
                    Ok(true) => return LoopControl::Break,
                    Ok(false) => {}
                    Err(e) => {
                        app.add_system_message(format!("Error: {e}"), SystemMessageKind::Error);
                    }
                }
            }
            LoopControl::Continue
        }
        AppAction::CycleMode => {
            app.set_mode(app.mode.next_mode());
            if let Err(e) = with_agent_mut(agent, |agent| {
                agent.set_mode(app.mode);
            }) {
                app.add_system_message(
                    format!("Failed to update mode: {e}"),
                    SystemMessageKind::Error,
                );
            }
            app.add_system_message(
                format!("Switched to {} mode", app.mode.display_name()),
                SystemMessageKind::Info,
            );
            LoopControl::Continue
        }
        AppAction::ToggleToolExpand => {
            app.toggle_tool_expand();
            LoopControl::Continue
        }

        AppAction::ScrollUp => {
            app.scroll_up();
            LoopControl::Continue
        }
        AppAction::ScrollDown => {
            app.scroll_down();
            LoopControl::Continue
        }
        AppAction::Interrupt => {
            // Properly cancel the agent task
            agent_task.borrow_mut().stop();
            app.stop_loading();
            app.add_interrupt();
            LoopControl::Continue
        }
        AppAction::Approve(response) => {
            // Send response back to the agent
            if let Some(tx) = pending_approval.take() {
                let feedback = if response == ApprovalResponse::No {
                    Some("User rejected the tool execution".to_string())
                } else {
                    None
                };

                // Send directly - oneshot::Sender::send is synchronous, not async.
                // Previously this used tokio::spawn which could cause deadlocks because
                // the spawned task runs on the global runtime while the agent awaits
                // the response in a LocalSet. Direct send immediately unblocks the agent.
                let _ = tx.send((response, feedback));
            }
            app.hide_approval();
            LoopControl::Continue
        }
        AppAction::OpenConfig => {
            // Open config app
            app.open_config_app();
            LoopControl::Continue
        }
    }
}

/// Handle a command. Returns true if should exit.
async fn handle_command(app: &mut App, agent: &mut Agent, cmd: &str) -> Result<bool> {
    let cmd = cmd.trim();

    // Handle shell commands (matching Mistral Vibe's BashOutputMessage style)
    if let Some(shell_cmd) = cmd.strip_prefix('!') {
        if shell_cmd.is_empty() {
            app.add_system_message(
                "No command provided after '!'".to_string(),
                SystemMessageKind::Error,
            );
            return Ok(false);
        }

        let cwd = app.config.effective_workdir().to_string_lossy().to_string();

        // Use tokio::process::Command to avoid blocking the async executor
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(shell_cmd)
            .current_dir(app.config.effective_workdir())
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let result = if !stdout.is_empty() {
                    stdout.to_string()
                } else if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    "(no output)".to_string()
                };
                let exit_code = output.status.code().unwrap_or(-1);
                app.add_bash_output(shell_cmd.to_string(), cwd, result, exit_code);
            }
            Err(e) => {
                app.add_system_message(format!("Command failed: {e}"), SystemMessageKind::Error);
            }
        }
        return Ok(false);
    }

    // Parse command
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let command = parts.first().map(|s| s.to_lowercase());

    match command.as_deref() {
        Some("/quit") | Some("/exit") | Some("/q") => {
            app.add_system_message("Bye!".to_string(), SystemMessageKind::Info);
            Ok(true)
        }
        Some("/clear") => {
            agent.clear_history().await?;
            app.messages.clear();
            app.add_system_message("Conversation cleared.".to_string(), SystemMessageKind::Info);
            Ok(false)
        }
        Some("/reset") => {
            agent.clear_history().await?;
            app.messages.clear();
            app.add_system_message(
                "Conversation reset. All history cleared.".to_string(),
                SystemMessageKind::Info,
            );
            Ok(false)
        }
        Some("/compact") => {
            app.add_system_message(
                "Compacting conversation...".to_string(),
                SystemMessageKind::Info,
            );
            agent.compact().await?;
            app.add_system_message("Compacted.".to_string(), SystemMessageKind::Info);
            Ok(false)
        }
        Some("/help") | Some("/?") => {
            let help = app.commands.get_help_text();
            app.add_system_message(help, SystemMessageKind::Info);
            Ok(false)
        }
        Some("/stats") | Some("/status") => {
            let stats = agent.stats().clone();
            let status = format!(
                "## Agent Statistics

- **Steps**: {}
- **Session Prompt Tokens**: {}
- **Session Completion Tokens**: {}
- **Session Total LLM Tokens**: {}
- **Last Turn Tokens**: {}
- **Cost**: ${:.4}",
                stats.steps,
                stats.session_prompt_tokens,
                stats.session_completion_tokens,
                stats.session_total_llm_tokens(),
                stats.last_turn_total_tokens(),
                stats.session_cost(),
            );
            app.add_system_message(status, SystemMessageKind::Info);
            Ok(false)
        }
        Some("/log") => {
            if let Some(log_path) = agent.log_file_path() {
                app.add_system_message(
                    format!(
                        "## Current Log File Path\n\n`{}`\n\nYou can send this file to share your interaction.",
                        log_path.display()
                    ),
                    SystemMessageKind::Info,
                );
            } else {
                app.add_system_message(
                    "Session logging is disabled or no log file created yet.".to_string(),
                    SystemMessageKind::Warning,
                );
            }
            Ok(false)
        }
        Some("/terminal-setup") => {
            // Detect terminal and provide setup instructions
            let terminal = std::env::var("TERM_PROGRAM").unwrap_or_default();
            let message = match terminal.as_str() {
                "iTerm.app" => {
                    "## iTerm2 Setup\n\nTo enable Shift+Enter for multiline input:\n\n1. Open iTerm2 Preferences (⌘,)\n2. Go to Keys → Key Bindings\n3. Click + to add a new binding\n4. Set Keyboard Shortcut to Shift+Enter\n5. Set Action to \"Send Text with vim Special Chars\"\n6. Enter: \\n\n\nRestart iTerm2 for changes to take effect."
                }
                "Apple_Terminal" => {
                    "## Terminal.app Setup\n\nMacOS Terminal has limited key binding support.\nConsider using iTerm2 or another terminal for better experience."
                }
                _ => {
                    "## Terminal Setup\n\nFor Shift+Enter support, check your terminal's documentation.\nMost modern terminals support custom key bindings.\n\nAlternatively, use Ctrl+J to insert newlines."
                }
            };
            app.add_system_message(message.to_string(), SystemMessageKind::Info);
            Ok(false)
        }
        Some("/config") | Some("/theme") | Some("/model") => {
            // Open config app
            app.open_config_app();
            Ok(false)
        }
        Some("/reload") => {
            match VibeConfig::load(None) {
                Ok(new_config) => {
                    app.config = new_config;
                    app.add_system_message(
                        "Configuration reloaded successfully.".to_string(),
                        SystemMessageKind::Info,
                    );
                }
                Err(e) => {
                    app.add_system_message(
                        format!("Failed to reload configuration: {}", e),
                        SystemMessageKind::Error,
                    );
                }
            }
            Ok(false)
        }
        Some("/session") => {
            let session_info = if let Some(session_id) = &app.session_id {
                format!(
                    "## Current Session

- **Session ID**: {}
- **Mode**: {}
- **Working Directory**: {}",
                    &session_id[..8.min(session_id.len())],
                    app.mode.display_name(),
                    app.config.effective_workdir().display()
                )
            } else {
                "No active session".to_string()
            };
            app.add_system_message(session_info, SystemMessageKind::Info);
            Ok(false)
        }
        Some("/update") => {
            // Check for updates manually
            if let Err(e) = app.check_version_updates().await {
                app.add_system_message(
                    format!("Failed to check for updates: {}", e),
                    SystemMessageKind::Error,
                );
            } else {
                app.add_system_message(
                    "Checked for updates. No new version available.".to_string(),
                    SystemMessageKind::Info,
                );
            }
            Ok(false)
        }
        Some(unknown) => {
            app.add_system_message(
                format!(
                    "Unknown command: {}. Type /help for available commands.",
                    unknown
                ),
                SystemMessageKind::Warning,
            );
            Ok(false)
        }
        None => Ok(false),
    }
}
