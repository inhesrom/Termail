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

use std::collections::HashMap;
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

    // Initialize logging to file (fresh log each session)
    let data_dir = config::data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let log_file = std::fs::File::create(data_dir.join("termail.log"))?;
    let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter("termail=debug")
        .init();

    // Terminal setup — enter alternate screen first, but delay mouse capture
    // until after the image protocol probe. EnableMouseCapture causes the
    // terminal to send mouse events via stdin, which corrupts the
    // stdin-based capability query in from_query_stdio().
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Probe terminal image protocol BEFORE mouse capture so stdin is clean.
    //
    // Ghostty requires per-cell diacritics on Kitty unicode placeholders.
    // Our vendored ratatui-image includes this fix, so override to Kitty.
    let image_picker = match ratatui_image::picker::Picker::from_query_stdio() {
        Ok(mut picker) => {
            tracing::info!(
                "Image picker: {:?} protocol, font_size={:?}",
                picker.protocol_type(),
                picker.font_size()
            );
            if is_ghostty() {
                tracing::info!("Ghostty detected — overriding to Kitty protocol");
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
            }
            picker
        }
        Err(e) => {
            tracing::warn!("Image protocol query failed ({}), using fallback", e);
            let mut picker = ratatui_image::picker::Picker::halfblocks();
            if is_ghostty() {
                tracing::info!("Ghostty detected (fallback) — using Kitty protocol");
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
            }
            picker
        }
    };

    // NOW enable mouse capture — stdin is no longer needed for protocol detection.
    execute!(terminal.backend_mut(), EnableMouseCapture)?;

    // Run the app
    let result = run(&mut terminal, image_picker).await;

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

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    image_picker: ratatui_image::picker::Picker,
) -> anyhow::Result<()> {
    tracing::info!("Termail starting");
    let mut app = App::new(image_picker);
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
    } else if app.has_accounts {
        // Account exists but backend creation failed (likely missing credentials
        // after migrating from keyring to file-based store). Show error and
        // prompt re-setup.
        tracing::warn!("Account configured but credentials missing — prompting re-setup");
        app.sync_status = app::SyncStatus::Error(
            "Credentials not found. Press X to reset account and re-enter your password.".into(),
        );
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

        // View: render current state, then apply animation effects on top.
        // Track whether animations are active BEFORE drawing: animation
        // effects overwrite buffer cells including Kitty protocol image
        // data (escape sequences + U+10EEEE placeholders). The Kitty
        // protocol uses a one-shot transmission that is consumed on the
        // first render, so if the animation overwrites it before ratatui
        // flushes the buffer, the image data never reaches the terminal.
        let had_animations = animations.has_active_effects();
        terminal.draw(|f| {
            ui::view(f, &app);
            animations.tick(f.buffer_mut());
        })?;
        // Invalidate the image protocol cache so that the next
        // animation-free frame creates fresh protocols that will
        // re-transmit their image data to the terminal.
        if had_animations {
            app.image_protocol_cache.borrow_mut().clear();
        }

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
            // TEMPORARILY DISABLED to debug image rendering
            // if commands.iter().any(|c| matches!(c, Command::FetchEmail(_))) {
            //     let preview_area = ratatui::layout::Rect {
            //         x: size.width * 30 / 100,
            //         y: 3,
            //         width: size.width * 70 / 100,
            //         height: size.height.saturating_sub(4),
            //     };
            //     animations.add_effect(
            //         ui::animations::AnimationManager::email_transition_effect(),
            //         preview_area,
            //     );
            // }

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

/// Returns true if the terminal is Ghostty (which needs Kitty protocol, not Sixel).
fn is_ghostty() -> bool {
    std::env::var("TERM_PROGRAM").is_ok_and(|v| v.eq_ignore_ascii_case("ghostty"))
}

/// Create an IMAP backend from the first account in config + stored credentials.
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
    tracing::info!("Retrieving stored credentials...");
    let password = match auth::token_store::get_token(&account.email) {
        Ok(Some(pw)) => pw,
        Ok(None) => {
            tracing::warn!("No stored credentials found for {}", account.email);
            return None;
        }
        Err(e) => {
            tracing::warn!("Failed to retrieve stored credentials: {}", e);
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
                        Ok(mut email) => {
                            // Fetch external images referenced in HTML body
                            if let Some(html) = &email.body_html {
                                let fetched = fetch_external_images(html).await;
                                email.inline_images.extend(fetched);
                            }
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
                        inline_images: std::collections::HashMap::new(),
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
            if let Some(backend) = backend {
                let backend = Arc::clone(backend);
                let tx = msg_tx.clone();
                tokio::spawn(async move {
                    tracing::info!("Search query: {}", query);
                    match backend.search_emails("INBOX", &query).await {
                        Ok(envelopes) => {
                            let _ = tx.send(Message::SearchResults(envelopes));
                        }
                        Err(e) => {
                            tracing::error!("Search failed: {}", e);
                            let _ = tx.send(Message::SyncError(format!("Search error: {}", e)));
                        }
                    }
                });
            } else {
                // Demo mode: local string filtering on current envelopes
                let q = query.to_lowercase();
                let filtered: Vec<_> = app
                    .envelopes
                    .iter()
                    .filter(|e| {
                        e.subject.to_lowercase().contains(&q)
                            || e.from_name.to_lowercase().contains(&q)
                            || e.from_address.to_lowercase().contains(&q)
                            || e.snippet.to_lowercase().contains(&q)
                    })
                    .cloned()
                    .collect();
                let _ = msg_tx.send(Message::SearchResults(filtered));
            }
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
                // Store password in credentials file
                if let Err(e) = auth::token_store::store_token(&email, &password) {
                    tracing::error!("Failed to store credentials: {}", e);
                    let _ = tx.send(Message::SetupError(format!("Credential store error: {}", e)));
                    return;
                }
                tracing::info!("Credentials saved to local store");

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

/// Extract `<img src="https://...">` URLs from HTML and fetch them concurrently.
/// Returns a map of URL -> image bytes for successfully fetched images.
async fn fetch_external_images(html: &str) -> HashMap<String, Vec<u8>> {
    // Extract URLs synchronously so `scraper::Html` (which is !Send) is
    // dropped before any `.await` points.
    let urls: Vec<String> = {
        let document = scraper::Html::parse_fragment(html);
        let img_selector = scraper::Selector::parse("img").unwrap();
        document
            .select(&img_selector)
            .filter_map(|el| el.value().attr("src"))
            .filter(|src| src.starts_with("https://") || src.starts_with("http://"))
            .map(String::from)
            .collect()
    };

    if urls.is_empty() {
        return HashMap::new();
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let fetches = urls.into_iter().map(|url| {
        let client = client.clone();
        async move {
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                    Ok(bytes) if bytes.len() <= 5 * 1024 * 1024 => {
                        tracing::debug!(
                            "Fetched external image ({} bytes): {}",
                            bytes.len(),
                            url
                        );
                        Some((url, bytes.to_vec()))
                    }
                    Ok(bytes) => {
                        tracing::debug!(
                            "External image too large ({} bytes), skipping: {}",
                            bytes.len(),
                            url
                        );
                        None
                    }
                    Err(e) => {
                        tracing::debug!("Failed to read image body: {} - {}", url, e);
                        None
                    }
                },
                Ok(resp) => {
                    tracing::debug!("External image HTTP {}: {}", resp.status(), url);
                    None
                }
                Err(e) => {
                    tracing::debug!("Failed to fetch external image: {} - {}", url, e);
                    None
                }
            }
        }
    });

    futures::future::join_all(fetches)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// Read log lines from the current session's log file.
/// Returns up to 1000 lines in reverse order (newest first).
fn load_log_lines() -> Vec<String> {
    let data_dir = match config::data_dir() {
        Ok(d) => d,
        Err(_) => return vec!["Failed to resolve data directory".into()],
    };

    let log_path = data_dir.join("termail.log");

    match std::fs::read_to_string(&log_path) {
        Ok(content) => {
            let all_lines: Vec<String> = content.lines().map(String::from).collect();
            let start = all_lines.len().saturating_sub(1000);
            all_lines[start..].iter().rev().cloned().collect()
        }
        Err(e) => vec![format!("Failed to read log file: {}", e)],
    }
}
