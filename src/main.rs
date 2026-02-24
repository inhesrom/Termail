#![allow(dead_code)]

mod app;
mod auth;
mod backend;
mod cache;
mod config;
mod event;
mod message;
mod models;
mod setup;
mod ui;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use app::{App, Command};
use backend::EmailBackend;
use backend::imap::{ImapBackend, ImapCredential};
use event::{EventHandler, InputMode};
use message::Message;

#[derive(Parser)]
#[command(name = "termail", about = "A terminal email client")]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Interactive account setup wizard
    Setup,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Route to setup wizard or TUI
    match cli.command {
        Some(CliCommand::Setup) => return setup::run_setup().await,
        None => {}
    }

    // Initialize logging to file
    let data_dir = config::data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "termail.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter("termail=debug")
        .init();

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let result = run(&mut terminal).await;

    // Terminal teardown (always runs, even on error)
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    tracing::info!("Termail starting");
    let mut app = App::new();
    let mut events = EventHandler::new(Duration::from_millis(16));
    let mut animations = ui::animations::AnimationManager::new();

    // Queue initial fade-in effect
    let term_size = terminal.size()?;
    let size = ratatui::layout::Rect::new(0, 0, term_size.width, term_size.height);
    animations.add_effect(
        ui::animations::AnimationManager::fade_in_effect(),
        size,
    );

    // Channel for async command results to send messages back
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Message>();

    // Create IMAP backend if accounts exist at startup
    let mut imap_backend: Option<Arc<ImapBackend>> = if app.has_accounts {
        create_backend()
    } else {
        None
    };

    // Trigger initial fetch if backend is ready
    if imap_backend.is_some() {
        tracing::info!("Starting initial inbox sync...");
        execute_command(Command::FetchEnvelopes, &app, imap_backend.as_ref(), &msg_tx);
    }

    while app.running {
        // Sync input mode with event handler (setup > compose > search > normal)
        let mode = if app.setup.is_some() {
            InputMode::Setup
        } else if app.compose.is_some() {
            InputMode::Compose
        } else if app.log_viewer.is_some() {
            InputMode::LogViewer
        } else if app.search_active {
            InputMode::Search
        } else {
            InputMode::Normal
        };
        events.set_mode(mode);

        // View: render current state, then apply animation effects on top
        terminal.draw(|f| {
            ui::view(f, &app);
            animations.tick(f.buffer_mut());
        })?;

        // Wait for next event/message (from events or async results)
        let msg = tokio::select! {
            msg = events.next() => msg,
            msg = msg_rx.recv() => msg,
        };

        if let Some(msg) = msg {
            // Detect scroll boundary hits for bounce animation
            let at_top = app.selected_index == 0;
            let at_bottom = !app.envelopes.is_empty()
                && app.selected_index == app.envelopes.len() - 1;

            let commands = app.update(msg.clone());

            // Trigger bounce effects at scroll boundaries
            match &msg {
                Message::SelectPrevious if at_top => {
                    animations.add_effect(
                        ui::animations::AnimationManager::bounce_effect(
                            ui::animations::BounceDirection::Up,
                        ),
                        size,
                    );
                }
                Message::SelectNext if at_bottom => {
                    animations.add_effect(
                        ui::animations::AnimationManager::bounce_effect(
                            ui::animations::BounceDirection::Down,
                        ),
                        size,
                    );
                }
                Message::Refresh => {
                    animations.add_effect(
                        ui::animations::AnimationManager::refresh_effect(),
                        size,
                    );
                }
                _ => {}
            }

            // Trigger email transition when switching emails
            if commands.iter().any(|c| matches!(c, Command::FetchEmail(_))) {
                let preview_area = ratatui::layout::Rect {
                    x: size.width * 30 / 100,
                    y: 3,
                    width: size.width * 70 / 100,
                    height: size.height.saturating_sub(4),
                };
                animations.add_effect(
                    ui::animations::AnimationManager::email_transition_effect(),
                    preview_area,
                );
            }

            for cmd in commands {
                execute_command(cmd, &app, imap_backend.as_ref(), &msg_tx);
            }

            // Destroy backend if account was reset
            if !app.has_accounts {
                imap_backend = None;
            }

            // If we have accounts but no backend yet (e.g. after setup), create it
            if app.has_accounts && imap_backend.is_none() {
                tracing::info!("Account configured, connecting to mail server...");
                imap_backend = create_backend();
                if imap_backend.is_some() {
                    // Update account_email from newly saved config
                    if let Ok(cfg) = config::load_config()
                        && let Some(acct) = cfg.accounts.first()
                    {
                        app.account_email = acct.email.clone();
                        tracing::info!("Starting inbox sync for {}...", acct.email);
                    }
                    execute_command(Command::FetchEnvelopes, &app, imap_backend.as_ref(), &msg_tx);
                }
            }
        }
    }

    Ok(())
}

/// Create an IMAP backend from the first account in config + keyring password.
fn create_backend() -> Option<Arc<ImapBackend>> {
    tracing::info!("Loading account configuration...");
    let config = match config::load_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!("Failed to load account configuration: {}", e);
            return None;
        }
    };
    let account = match config.accounts.first() {
        Some(acct) => acct,
        None => {
            tracing::warn!("No accounts configured");
            return None;
        }
    };
    tracing::info!("Found account: {}", account.email);
    tracing::info!("Retrieving credentials from keyring...");
    let password = match auth::token_store::get_token(&account.email) {
        Ok(Some(pw)) => pw,
        Ok(None) => {
            tracing::warn!("No credentials found in keyring for {}", account.email);
            return None;
        }
        Err(e) => {
            tracing::warn!("Failed to retrieve credentials from keyring: {}", e);
            return None;
        }
    };
    let backend = ImapBackend::new(
        account.email.clone(),
        ImapCredential::Password(password),
    );
    tracing::info!("IMAP backend created for {}", account.email);
    Some(Arc::new(backend))
}

/// Execute a command (side effect). For async commands, spawns a task that
/// sends the result back via msg_tx.
fn execute_command(
    cmd: Command,
    app: &App,
    backend: Option<&Arc<ImapBackend>>,
    msg_tx: &mpsc::UnboundedSender<Message>,
) {
    match cmd {
        Command::Quit | Command::None => {}
        Command::LoadLogs => {
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                let lines = load_log_lines();
                let _ = tx.send(Message::LogViewerLoaded(lines));
            });
        }
        Command::FetchEmail(uid) => {
            if let Some(backend) = backend {
                let backend = Arc::clone(backend);
                let tx = msg_tx.clone();
                tokio::spawn(async move {
                    match backend.fetch_email("INBOX", uid).await {
                        Ok(email) => {
                            let _ = tx.send(Message::EmailFetched(Box::new(email)));
                        }
                        Err(e) => {
                            tracing::error!("Failed to fetch email uid={}: {}", uid, e);
                            let _ = tx.send(Message::SyncError(format!("Fetch error: {}", e)));
                        }
                    }
                });
            } else {
                // No backend — build placeholder from envelope data
                if let Some(env) = app.envelopes.iter().find(|e| e.uid == uid) {
                    let email = models::email::Email {
                        uid: env.uid,
                        message_id: format!("<{}@placeholder>", env.uid),
                        from_name: env.from_name.clone(),
                        from_address: env.from_address.clone(),
                        to: vec![],
                        cc: vec![],
                        subject: env.subject.clone(),
                        date: env.date,
                        body_text: env.snippet.clone(),
                        body_html: None,
                        attachments: vec![],
                        is_read: env.is_read,
                        is_starred: env.is_starred,
                    };
                    let _ = msg_tx.send(Message::EmailFetched(Box::new(email)));
                }
            }
        }
        Command::FetchEnvelopes => {
            if let Some(backend) = backend {
                tracing::info!("Fetching inbox...");
                let backend = Arc::clone(backend);
                let tx = msg_tx.clone();
                tokio::spawn(async move {
                    match backend.fetch_envelopes("INBOX", None, Some(50)).await {
                        Ok(envelopes) => {
                            let _ = tx.send(Message::EnvelopesFetched(envelopes));
                        }
                        Err(e) => {
                            tracing::error!("Failed to fetch envelopes: {}", e);
                            let _ = tx.send(Message::SyncError(format!("Sync error: {}", e)));
                        }
                    }
                });
            } else {
                let _ = msg_tx.send(Message::SyncComplete);
            }
        }
        Command::SendEmail {
            to,
            cc,
            subject,
            body,
        } => {
            if let Some(backend) = backend {
                let backend = Arc::clone(backend);
                let tx = msg_tx.clone();
                tokio::spawn(async move {
                    match backend.send_email(&to, &cc, &subject, &body).await {
                        Ok(()) => {
                            tracing::info!("Email sent to={}", to);
                            let _ = tx.send(Message::SyncComplete);
                        }
                        Err(e) => {
                            tracing::error!("Failed to send email: {}", e);
                            let _ = tx.send(Message::SyncError(format!("Send error: {}", e)));
                        }
                    }
                });
            } else {
                tracing::info!("Would send email to={}, cc={}, subject={}", to, cc, subject);
                let _ = msg_tx.send(Message::SyncComplete);
            }
        }
        Command::DeleteEmail(uid) => {
            if let Some(backend) = backend {
                let backend = Arc::clone(backend);
                let tx = msg_tx.clone();
                tokio::spawn(async move {
                    match backend.delete_email("INBOX", uid).await {
                        Ok(()) => {
                            tracing::info!("Deleted email uid={}", uid);
                            let _ = tx.send(Message::SyncComplete);
                        }
                        Err(e) => {
                            tracing::error!("Failed to delete email uid={}: {}", uid, e);
                            let _ = tx.send(Message::SyncError(format!("Delete error: {}", e)));
                        }
                    }
                });
            } else {
                tracing::info!("Would delete email uid={}", uid);
                let _ = msg_tx.send(Message::SyncComplete);
            }
        }
        Command::ArchiveEmail(uid) => {
            if let Some(backend) = backend {
                let backend = Arc::clone(backend);
                let tx = msg_tx.clone();
                tokio::spawn(async move {
                    match backend.archive_email("INBOX", uid).await {
                        Ok(()) => {
                            tracing::info!("Archived email uid={}", uid);
                            let _ = tx.send(Message::SyncComplete);
                        }
                        Err(e) => {
                            tracing::error!("Failed to archive email uid={}: {}", uid, e);
                            let _ = tx.send(Message::SyncError(format!("Archive error: {}", e)));
                        }
                    }
                });
            } else {
                tracing::info!("Would archive email uid={}", uid);
                let _ = msg_tx.send(Message::SyncComplete);
            }
        }
        Command::SetFlag { uid, flag, value } => {
            if let Some(backend) = backend {
                let backend = Arc::clone(backend);
                let tx = msg_tx.clone();
                let flag_str = flag.clone();
                let email_flag = match flag.as_str() {
                    "seen" => backend::EmailFlag::Seen,
                    "starred" => backend::EmailFlag::Starred,
                    "deleted" => backend::EmailFlag::Deleted,
                    _ => {
                        tracing::warn!("Unknown flag: {}", flag);
                        return;
                    }
                };
                tokio::spawn(async move {
                    match backend.set_flag("INBOX", uid, email_flag, value).await {
                        Ok(()) => {
                            tracing::info!("Set flag {}={} on uid={}", flag_str, value, uid);
                            let _ = tx.send(Message::SyncComplete);
                        }
                        Err(e) => {
                            tracing::error!("Failed to set flag on uid={}: {}", uid, e);
                            let _ = tx.send(Message::SyncError(format!("Flag error: {}", e)));
                        }
                    }
                });
            } else {
                tracing::info!("Would set flag {}={} on uid={}", flag, value, uid);
                let _ = msg_tx.send(Message::SyncComplete);
            }
        }
        Command::Search(query) => {
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                // Search index (Tantivy) not wired yet — return empty results.
                tracing::info!("Search query: {}", query);
                let _ = tx.send(Message::SearchResults(vec![]));
            });
        }
        Command::ResetAccount { email } => {
            if let Err(e) = auth::token_store::delete_token(&email) {
                tracing::error!("Failed to delete token for {}: {}", email, e);
            }
            if let Err(e) = setup::remove_account_from_config() {
                tracing::error!("Failed to remove account from config: {}", e);
            }
            tracing::info!("Account reset for {}", email);
        }
        Command::SaveAccount { name, email, password } => {
            tracing::info!("Saving account {}...", email);
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                // Store password in OS keyring
                if let Err(e) = auth::token_store::store_token(&email, &password) {
                    tracing::error!("Failed to store password in keyring: {}", e);
                    let _ = tx.send(Message::SetupError(format!("Keyring error: {}", e)));
                    return;
                }
                tracing::info!("Credentials stored in keyring");

                tracing::info!("Writing account to config file...");
                let account = models::account::Account {
                    name,
                    email,
                    provider: models::account::Provider::Gmail,
                    client_id: None,
                    client_secret: None,
                };
                match setup::append_account_to_config(&account) {
                    Ok(()) => {
                        tracing::info!("Account saved: {}", account.email);
                        let _ = tx.send(Message::SetupComplete);
                    }
                    Err(e) => {
                        tracing::error!("Failed to save account: {}", e);
                        let _ = tx.send(Message::SetupError(e.to_string()));
                    }
                }
            });
        }
    }
}

/// Read recent log lines from the termail log file on disk.
/// Returns up to 1000 lines in reverse order (newest first).
fn load_log_lines() -> Vec<String> {
    let data_dir = match config::data_dir() {
        Ok(d) => d,
        Err(_) => return vec!["Failed to resolve data directory".into()],
    };

    // Try today's log file first
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_path = data_dir.join(format!("termail.log.{}", today));

    let log_path = if today_path.exists() {
        today_path
    } else {
        // Fall back to most recent termail.log.* file
        let mut entries: Vec<_> = std::fs::read_dir(&data_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("termail.log")
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        match entries.first() {
            Some(e) => e.path(),
            None => return vec!["No log files found".into()],
        }
    };

    match std::fs::read_to_string(&log_path) {
        Ok(content) => {
            let all_lines: Vec<String> = content.lines().map(String::from).collect();
            let start = all_lines.len().saturating_sub(1000);
            all_lines[start..].iter().rev().cloned().collect()
        }
        Err(e) => vec![format!("Failed to read log file: {}", e)],
    }
}
