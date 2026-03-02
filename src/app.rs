use std::cell::RefCell;
use std::collections::HashMap;

use chrono::Local;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::message::Message;
use crate::models::email::Email;
use crate::models::envelope::Envelope;

/// Which pane currently has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    InboxList,
    EmailPreview,
}

/// Overlay mode for compose/reply.
#[derive(Debug, Clone)]
pub enum ComposeMode {
    New,
    Reply { to: String, subject: String, quoted_body: String },
    ReplyAll { to: Vec<String>, cc: Vec<String>, subject: String, quoted_body: String },
    Forward { subject: String, body: String },
}

/// Compose form state.
#[derive(Debug, Clone)]
pub struct ComposeState {
    pub mode: ComposeMode,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    pub active_field: ComposeField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeField {
    To,
    Cc,
    Subject,
    Body,
}

/// Which field is active in a Gmail setup form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmailSetupField {
    Name,
    Email,
    Password,
    Submit,
}

/// Which field is active in an Outlook setup form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlookSetupField {
    Name,
    Email,
    Submit,
}

/// Provider choice in the provider selection screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderOption {
    Gmail,
    Outlook,
}

/// Items shown in the account list.
#[derive(Debug, Clone)]
pub struct AccountListItem {
    pub name: String,
    pub email: String,
    pub provider: crate::models::account::Provider,
}

/// The current phase of the multi-step setup flow.
#[derive(Debug, Clone)]
pub enum SetupPhase {
    /// Shows existing accounts + "Add Account" + "Done".
    AccountList {
        accounts: Vec<AccountListItem>,
        /// Index into: [account0, account1, ..., AddAccount, Done]
        selected: usize,
    },
    /// Choose Gmail or Outlook.
    ProviderSelect {
        selected: ProviderOption,
    },
    /// Gmail-specific fields: name, email, app password.
    GmailFields {
        name: String,
        email: String,
        password: String,
        active_field: GmailSetupField,
    },
    /// Outlook-specific fields: name, email (no password).
    OutlookFields {
        name: String,
        email: String,
        active_field: OutlookSetupField,
    },
    /// Displaying device code, waiting for user to authenticate in browser.
    OutlookDeviceCode {
        name: String,
        email: String,
        verification_uri: String,
        user_code: String,
    },
}

/// Setup overlay state.
#[derive(Debug, Clone)]
pub struct SetupState {
    pub phase: SetupPhase,
    pub status: Option<String>,
}

/// Log level filter for the log viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilter {
    All,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogFilter {
    /// Cycle to the next filter level.
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Debug,
            Self::Debug => Self::Info,
            Self::Info => Self::Warn,
            Self::Warn => Self::Error,
            Self::Error => Self::All,
        }
    }

    /// Check if a log line passes the filter.
    pub fn matches(self, line: &str) -> bool {
        match self {
            Self::All => true,
            Self::Debug => true,
            Self::Info => !line.contains(" DEBUG "),
            Self::Warn => line.contains(" WARN ") || line.contains(" ERROR "),
            Self::Error => line.contains(" ERROR "),
        }
    }

    /// Display label for the title bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Debug => "DEBUG+",
            Self::Info => "INFO+",
            Self::Warn => "WARN+",
            Self::Error => "ERROR",
        }
    }
}

/// Log viewer overlay state.
#[derive(Debug, Clone)]
pub struct LogViewerState {
    pub lines: Vec<String>,
    pub scroll: u16,
    pub filter_level: LogFilter,
}

impl LogViewerState {
    pub fn filtered_lines(&self) -> Vec<&String> {
        self.lines.iter().filter(|l| self.filter_level.matches(l)).collect()
    }
}

/// Commands returned by update() for side effects.
#[derive(Debug)]
pub enum Command {
    FetchEnvelopes,
    FetchEmail(u32),
    SendEmail { to: String, cc: String, subject: String, body: String },
    DeleteEmail(u32),
    ArchiveEmail(u32),
    SetFlag { uid: u32, flag: String, value: bool },
    Search(String),
    SaveAccount { name: String, email: String, password: String },
    SaveOutlookAccount { name: String, email: String, client_id: Option<String> },
    StartOutlookAuth { name: String, email: String, client_id: Option<String> },
    RemoveAccount { email: String },
    ResetAccount { email: String },
    LoadLogs,
    Quit,
    None,
}

/// Sync status indicator.
#[derive(Debug, Clone)]
pub enum SyncStatus {
    Idle,
    Syncing,
    Error(String),
    LastSync(chrono::DateTime<Local>),
}

/// The single source of truth for all application state (Elm Architecture Model).
pub struct App {
    pub envelopes: Vec<Envelope>,
    pub selected_index: usize,
    pub selected_email: Option<Email>,
    pub preview_scroll: u16,
    pub active_pane: Pane,
    pub search_active: bool,
    pub search_query: String,
    pub compose: Option<ComposeState>,
    pub setup: Option<SetupState>,
    pub log_viewer: Option<LogViewerState>,
    pub pre_search_envelopes: Option<Vec<Envelope>>,
    pub sync_status: SyncStatus,
    pub account_email: String,
    pub has_accounts: bool,
    pub running: bool,
    pub terminal_size: (u16, u16),
    pub image_picker: ratatui_image::picker::Picker,
    /// Cached `StatefulProtocol` instances for inline images, keyed by CID.
    /// Uses `RefCell` for interior mutability so the render chain can work
    /// with `&App`.
    pub image_protocol_cache: RefCell<HashMap<String, ratatui_image::protocol::StatefulProtocol>>,
}

impl App {
    pub fn new(image_picker: ratatui_image::picker::Picker) -> Self {
        let config = crate::config::load_config().ok();
        let has_accounts = config
            .as_ref()
            .map(|c| !c.accounts.is_empty())
            .unwrap_or(false);

        let setup = if !has_accounts {
            Some(SetupState {
                phase: SetupPhase::ProviderSelect {
                    selected: ProviderOption::Gmail,
                },
                status: None,
            })
        } else {
            None
        };

        let account_email = config
            .as_ref()
            .and_then(|c| c.accounts.first())
            .map(|a| a.email.clone())
            .unwrap_or_else(|| "user@gmail.com".to_string());

        let (envelopes, selected_email, sync_status) = if has_accounts {
            (vec![], None, SyncStatus::Syncing)
        } else {
            (dummy_envelopes(), Some(dummy_email()), SyncStatus::LastSync(Local::now()))
        };

        Self {
            envelopes,
            selected_index: 0,
            selected_email,
            preview_scroll: 0,
            active_pane: Pane::InboxList,
            search_active: false,
            search_query: String::new(),
            compose: None,
            setup,
            log_viewer: None,
            pre_search_envelopes: None,
            sync_status,
            account_email,
            has_accounts,
            running: true,
            terminal_size: (80, 24),
            image_picker,
            image_protocol_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Pure update function: takes a Message and returns commands (side effects).
    pub fn update(&mut self, msg: Message) -> Vec<Command> {
        match msg {
            // -- Navigation --
            Message::SelectNext => {
                if self.active_pane == Pane::EmailPreview {
                    self.preview_scroll = self.preview_scroll.saturating_add(1);
                } else if !self.envelopes.is_empty() {
                    self.selected_index = (self.selected_index + 1).min(self.envelopes.len() - 1);
                    self.preview_scroll = 0;
                }
            }
            Message::SelectPrevious => {
                if self.active_pane == Pane::EmailPreview {
                    self.preview_scroll = self.preview_scroll.saturating_sub(1);
                } else {
                    self.selected_index = self.selected_index.saturating_sub(1);
                    self.preview_scroll = 0;
                }
            }
            Message::OpenSelected => {
                if let Some(env) = self.envelopes.get_mut(self.selected_index) {
                    let uid = env.uid;
                    let was_unread = !env.is_read;
                    env.is_read = true;
                    self.active_pane = Pane::EmailPreview;
                    self.preview_scroll = 0;
                    let mut cmds = vec![Command::FetchEmail(uid)];
                    if was_unread {
                        cmds.push(Command::SetFlag { uid, flag: "seen".into(), value: true });
                    }
                    return cmds;
                }
            }
            Message::SwitchPane => {
                self.active_pane = match self.active_pane {
                    Pane::InboxList => Pane::EmailPreview,
                    Pane::EmailPreview => Pane::InboxList,
                };
            }
            Message::ScrollPreviewDown => {
                self.preview_scroll = self.preview_scroll.saturating_add(1);
            }
            Message::ScrollPreviewUp => {
                self.preview_scroll = self.preview_scroll.saturating_sub(1);
            }

            // -- Mouse --
            Message::MouseClick(col, row) => {
                // Skip mouse handling when overlays are open
                if self.no_overlay_active() {
                    let (inbox_area, preview_area) = self.compute_pane_areas();
                    let pos = ratatui::layout::Position { x: col, y: row };

                    if inbox_area.contains(pos) {
                        self.active_pane = Pane::InboxList;
                        // Each list item is 2 lines tall; account for border (1 row at top)
                        let inner_y = row.saturating_sub(inbox_area.y + 1);
                        let inner_height = inbox_area.height.saturating_sub(2); // borders

                        // Estimate which items are visible based on selected_index
                        let visible_items = (inner_height / 2) as usize;
                        let scroll_offset = if self.selected_index >= visible_items {
                            self.selected_index - visible_items + 1
                        } else {
                            0
                        };

                        let clicked_item = scroll_offset + (inner_y / 2) as usize;
                        if clicked_item < self.envelopes.len() {
                            self.selected_index = clicked_item;
                            return self.update(Message::OpenSelected);
                        }
                    } else if preview_area.contains(pos) {
                        self.active_pane = Pane::EmailPreview;
                    }
                }
            }
            Message::MouseScrollDown(col, row) => {
                if self.no_overlay_active() {
                    let (inbox_area, preview_area) = self.compute_pane_areas();
                    let pos = ratatui::layout::Position { x: col, y: row };

                    if inbox_area.contains(pos) {
                        if !self.envelopes.is_empty() {
                            self.selected_index =
                                (self.selected_index + 1).min(self.envelopes.len() - 1);
                            self.preview_scroll = 0;
                        }
                    } else if preview_area.contains(pos) {
                        self.preview_scroll = self.preview_scroll.saturating_add(3);
                    }
                }
            }
            Message::MouseScrollUp(col, row) => {
                if self.no_overlay_active() {
                    let (inbox_area, preview_area) = self.compute_pane_areas();
                    let pos = ratatui::layout::Position { x: col, y: row };

                    if inbox_area.contains(pos) {
                        self.selected_index = self.selected_index.saturating_sub(1);
                        self.preview_scroll = 0;
                    } else if preview_area.contains(pos) {
                        self.preview_scroll = self.preview_scroll.saturating_sub(3);
                    }
                }
            }

            // -- Search --
            Message::ToggleSearch => {
                self.search_active = !self.search_active;
                if !self.search_active {
                    self.search_query.clear();
                    if let Some(original) = self.pre_search_envelopes.take() {
                        self.envelopes = original;
                        self.selected_index = 0;
                        self.selected_email = None;
                    }
                }
            }
            Message::SearchInput(ch) => {
                if self.search_active {
                    self.search_query.push(ch);
                }
            }
            Message::SearchBackspace => {
                if self.search_active {
                    self.search_query.pop();
                }
            }
            Message::SearchSubmit => {
                if !self.search_query.is_empty() {
                    return vec![Command::Search(self.search_query.clone())];
                }
            }
            Message::SearchClear => {
                self.search_active = false;
                self.search_query.clear();
                if let Some(original) = self.pre_search_envelopes.take() {
                    self.envelopes = original;
                    self.selected_index = 0;
                    self.selected_email = None;
                }
            }

            // -- Compose --
            Message::OpenCompose => {
                self.compose = Some(ComposeState {
                    mode: ComposeMode::New,
                    to: String::new(),
                    cc: String::new(),
                    subject: String::new(),
                    body: String::new(),
                    active_field: ComposeField::To,
                });
            }
            Message::OpenReply => {
                if let Some(email) = &self.selected_email {
                    let subject = reply_subject(&email.subject);
                    self.compose = Some(ComposeState {
                        mode: ComposeMode::Reply {
                            to: email.from_address.clone(),
                            subject: subject.clone(),
                            quoted_body: email.body_text.clone(),
                        },
                        to: email.from_address.clone(),
                        cc: String::new(),
                        subject,
                        body: String::new(),
                        active_field: ComposeField::Body,
                    });
                }
            }
            Message::OpenReplyAll => {
                if let Some(email) = &self.selected_email {
                    let subject = reply_subject(&email.subject);
                    self.compose = Some(ComposeState {
                        mode: ComposeMode::ReplyAll {
                            to: email.to.clone(),
                            cc: email.cc.clone(),
                            subject: subject.clone(),
                            quoted_body: email.body_text.clone(),
                        },
                        to: email.from_address.clone(),
                        cc: email.cc.join(", "),
                        subject,
                        body: String::new(),
                        active_field: ComposeField::Body,
                    });
                }
            }
            Message::OpenForward => {
                if let Some(email) = &self.selected_email {
                    let subject = format!("Fwd: {}", email.subject);
                    self.compose = Some(ComposeState {
                        mode: ComposeMode::Forward {
                            subject: subject.clone(),
                            body: email.body_text.clone(),
                        },
                        to: String::new(),
                        cc: String::new(),
                        subject,
                        body: format!(
                            "\n---------- Forwarded message ----------\nFrom: {} <{}>\nDate: {}\nSubject: {}\n\n{}",
                            email.from_name, email.from_address,
                            email.date.format("%b %d, %Y %l:%M %p"),
                            email.subject, email.body_text
                        ),
                        active_field: ComposeField::To,
                    });
                }
            }
            Message::ComposeInput(ch) => {
                if let Some(compose) = &mut self.compose {
                    active_compose_field_mut(compose).push(ch);
                }
            }
            Message::ComposeBackspace => {
                if let Some(compose) = &mut self.compose {
                    active_compose_field_mut(compose).pop();
                }
            }
            Message::ComposeNewline => {
                if let Some(compose) = &mut self.compose
                    && compose.active_field == ComposeField::Body
                {
                    compose.body.push('\n');
                }
            }
            Message::ComposeTabField => {
                if let Some(compose) = &mut self.compose {
                    compose.active_field = match compose.active_field {
                        ComposeField::To => ComposeField::Cc,
                        ComposeField::Cc => ComposeField::Subject,
                        ComposeField::Subject => ComposeField::Body,
                        ComposeField::Body => ComposeField::To,
                    };
                }
            }
            Message::ComposeSend => {
                if let Some(compose) = self.compose.take() {
                    return vec![Command::SendEmail {
                        to: compose.to,
                        cc: compose.cc,
                        subject: compose.subject,
                        body: compose.body,
                    }];
                }
            }
            Message::ComposeCancel => {
                self.compose = None;
            }

            // -- Setup --
            Message::OpenSetup => {
                let accounts = load_account_list();
                if accounts.is_empty() {
                    self.setup = Some(SetupState {
                        phase: SetupPhase::ProviderSelect {
                            selected: ProviderOption::Gmail,
                        },
                        status: None,
                    });
                } else {
                    self.setup = Some(SetupState {
                        phase: SetupPhase::AccountList {
                            accounts,
                            selected: 0,
                        },
                        status: None,
                    });
                }
            }
            Message::SetupInput(ch) => {
                if let Some(setup) = &mut self.setup {
                    match &mut setup.phase {
                        SetupPhase::GmailFields { name, email, password, active_field } => {
                            match active_field {
                                GmailSetupField::Name => name.push(ch),
                                GmailSetupField::Email => email.push(ch),
                                GmailSetupField::Password => password.push(ch),
                                GmailSetupField::Submit => {}
                            }
                        }
                        SetupPhase::OutlookFields { name, email, active_field } => {
                            match active_field {
                                OutlookSetupField::Name => name.push(ch),
                                OutlookSetupField::Email => email.push(ch),
                                OutlookSetupField::Submit => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::SetupBackspace => {
                if let Some(setup) = &mut self.setup {
                    match &mut setup.phase {
                        SetupPhase::GmailFields { name, email, password, active_field } => {
                            match active_field {
                                GmailSetupField::Name => { name.pop(); }
                                GmailSetupField::Email => { email.pop(); }
                                GmailSetupField::Password => { password.pop(); }
                                GmailSetupField::Submit => {}
                            }
                        }
                        SetupPhase::OutlookFields { name, email, active_field } => {
                            match active_field {
                                OutlookSetupField::Name => { name.pop(); }
                                OutlookSetupField::Email => { email.pop(); }
                                OutlookSetupField::Submit => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::SetupTabField => {
                if let Some(setup) = &mut self.setup {
                    match &mut setup.phase {
                        SetupPhase::GmailFields { active_field, .. } => {
                            *active_field = match active_field {
                                GmailSetupField::Name => GmailSetupField::Email,
                                GmailSetupField::Email => GmailSetupField::Password,
                                GmailSetupField::Password => GmailSetupField::Submit,
                                GmailSetupField::Submit => GmailSetupField::Name,
                            };
                        }
                        SetupPhase::OutlookFields { active_field, .. } => {
                            *active_field = match active_field {
                                OutlookSetupField::Name => OutlookSetupField::Email,
                                OutlookSetupField::Email => OutlookSetupField::Submit,
                                OutlookSetupField::Submit => OutlookSetupField::Name,
                            };
                        }
                        _ => {}
                    }
                }
            }
            Message::SetupSubmit => {
                if let Some(setup) = &mut self.setup {
                    match &setup.phase {
                        SetupPhase::GmailFields { name, email, password, .. } => {
                            if name.is_empty() || email.is_empty() || password.is_empty() {
                                setup.status = Some("All fields are required".into());
                            } else {
                                return vec![Command::SaveAccount {
                                    name: name.clone(),
                                    email: email.clone(),
                                    password: password.clone(),
                                }];
                            }
                        }
                        SetupPhase::OutlookFields { name, email, .. } => {
                            if name.is_empty() || email.is_empty() {
                                setup.status = Some("Name and email are required".into());
                            } else {
                                let name = name.clone();
                                let email = email.clone();
                                return vec![Command::StartOutlookAuth {
                                    name,
                                    email,
                                    client_id: None,
                                }];
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::SetupEnter => {
                if let Some(setup) = &mut self.setup {
                    setup.status = None;
                    match &mut setup.phase {
                        SetupPhase::AccountList { accounts, selected } => {
                            let num_accounts = accounts.len();
                            if *selected == num_accounts {
                                // "Add Account" button
                                setup.phase = SetupPhase::ProviderSelect {
                                    selected: ProviderOption::Gmail,
                                };
                            } else if *selected == num_accounts + 1 {
                                // "Done" button
                                self.setup = None;
                                return vec![Command::None];
                            } else {
                                // Selected an existing account — could show details or remove
                                // For now, treat enter on an account as remove
                                let email = accounts[*selected].email.clone();
                                return vec![Command::RemoveAccount { email }];
                            }
                        }
                        SetupPhase::ProviderSelect { selected } => {
                            match selected {
                                ProviderOption::Gmail => {
                                    setup.phase = SetupPhase::GmailFields {
                                        name: String::new(),
                                        email: String::new(),
                                        password: String::new(),
                                        active_field: GmailSetupField::Name,
                                    };
                                }
                                ProviderOption::Outlook => {
                                    setup.phase = SetupPhase::OutlookFields {
                                        name: String::new(),
                                        email: String::new(),
                                        active_field: OutlookSetupField::Name,
                                    };
                                }
                            }
                        }
                        SetupPhase::GmailFields { name, email, password, active_field } => {
                            if *active_field == GmailSetupField::Submit {
                                if name.is_empty() || email.is_empty() || password.is_empty() {
                                    setup.status = Some("All fields are required".into());
                                } else {
                                    return vec![Command::SaveAccount {
                                        name: name.clone(),
                                        email: email.clone(),
                                        password: password.clone(),
                                    }];
                                }
                            } else {
                                *active_field = match active_field {
                                    GmailSetupField::Name => GmailSetupField::Email,
                                    GmailSetupField::Email => GmailSetupField::Password,
                                    GmailSetupField::Password => GmailSetupField::Submit,
                                    GmailSetupField::Submit => GmailSetupField::Name,
                                };
                            }
                        }
                        SetupPhase::OutlookFields { name, email, active_field } => {
                            if *active_field == OutlookSetupField::Submit {
                                if name.is_empty() || email.is_empty() {
                                    setup.status = Some("Name and email are required".into());
                                } else {
                                    return vec![Command::StartOutlookAuth {
                                        name: name.clone(),
                                        email: email.clone(),
                                        client_id: None,
                                    }];
                                }
                            } else {
                                *active_field = match active_field {
                                    OutlookSetupField::Name => OutlookSetupField::Email,
                                    OutlookSetupField::Email => OutlookSetupField::Submit,
                                    OutlookSetupField::Submit => OutlookSetupField::Name,
                                };
                            }
                        }
                        SetupPhase::OutlookDeviceCode { .. } => {
                            // Waiting for auth — enter does nothing
                        }
                    }
                }
            }
            Message::SetupUp => {
                if let Some(setup) = &mut self.setup {
                    match &mut setup.phase {
                        SetupPhase::AccountList { accounts: _, selected } => {
                            if *selected > 0 {
                                *selected -= 1;
                            }
                        }
                        SetupPhase::ProviderSelect { selected } => {
                            *selected = ProviderOption::Gmail;
                        }
                        _ => {}
                    }
                }
            }
            Message::SetupDown => {
                if let Some(setup) = &mut self.setup {
                    match &mut setup.phase {
                        SetupPhase::AccountList { accounts, selected } => {
                            let max = accounts.len() + 1; // accounts + AddAccount + Done
                            if *selected < max {
                                *selected += 1;
                            }
                        }
                        SetupPhase::ProviderSelect { selected } => {
                            *selected = ProviderOption::Outlook;
                        }
                        _ => {}
                    }
                }
            }
            Message::SetupBack => {
                if let Some(setup) = &mut self.setup {
                    let has_accounts = load_account_list().len() > 0;
                    match &setup.phase {
                        SetupPhase::AccountList { .. } => {
                            // Can't go back from account list
                            self.setup = None;
                        }
                        SetupPhase::ProviderSelect { .. } => {
                            if has_accounts {
                                let accounts = load_account_list();
                                setup.phase = SetupPhase::AccountList { accounts, selected: 0 };
                            } else {
                                self.setup = None;
                            }
                        }
                        SetupPhase::GmailFields { .. } | SetupPhase::OutlookFields { .. } => {
                            setup.phase = SetupPhase::ProviderSelect {
                                selected: ProviderOption::Gmail,
                            };
                            setup.status = None;
                        }
                        SetupPhase::OutlookDeviceCode { .. } => {
                            setup.phase = SetupPhase::ProviderSelect {
                                selected: ProviderOption::Outlook,
                            };
                            setup.status = None;
                        }
                    }
                }
            }
            Message::SetupCancel => {
                // Esc: go back one step, or close
                if let Some(setup) = &self.setup {
                    match &setup.phase {
                        SetupPhase::AccountList { .. } => {
                            self.setup = None;
                        }
                        SetupPhase::ProviderSelect { .. } => {
                            let accounts = load_account_list();
                            if accounts.is_empty() {
                                self.setup = None;
                            } else {
                                self.setup = Some(SetupState {
                                    phase: SetupPhase::AccountList { accounts, selected: 0 },
                                    status: None,
                                });
                            }
                        }
                        _ => {
                            return self.update(Message::SetupBack);
                        }
                    }
                }
            }
            Message::SetupDeviceCodeReceived { verification_uri, user_code } => {
                if let Some(setup) = &mut self.setup {
                    // Capture name/email from OutlookFields before transitioning
                    let (name, email) = match &setup.phase {
                        SetupPhase::OutlookFields { name, email, .. } => {
                            (name.clone(), email.clone())
                        }
                        SetupPhase::OutlookDeviceCode { name, email, .. } => {
                            (name.clone(), email.clone())
                        }
                        _ => (String::new(), String::new()),
                    };
                    setup.phase = SetupPhase::OutlookDeviceCode {
                        name,
                        email,
                        verification_uri,
                        user_code,
                    };
                    setup.status = None;
                }
            }
            Message::SetupOutlookAuthComplete => {
                if let Some(setup) = &self.setup {
                    if let SetupPhase::OutlookDeviceCode { name, email, .. } = &setup.phase {
                        let name = name.clone();
                        let email = email.clone();
                        return vec![Command::SaveOutlookAccount {
                            name,
                            email,
                            client_id: None,
                        }];
                    }
                }
            }
            Message::SetupRemoveAccount(index) => {
                let accounts = load_account_list();
                if let Some(acct) = accounts.get(index) {
                    let email = acct.email.clone();
                    return vec![Command::RemoveAccount { email }];
                }
            }
            Message::SetupComplete => {
                // Reload account list — if still in setup, go back to account list
                let accounts = load_account_list();
                self.has_accounts = !accounts.is_empty();
                if self.has_accounts {
                    // Update account_email from config
                    if let Ok(cfg) = crate::config::load_config()
                        && let Some(acct) = cfg.accounts.first()
                    {
                        self.account_email = acct.email.clone();
                    }
                }
                if accounts.is_empty() {
                    self.setup = None;
                } else {
                    self.setup = Some(SetupState {
                        phase: SetupPhase::AccountList { accounts, selected: 0 },
                        status: Some("Account saved!".into()),
                    });
                }
                self.envelopes.clear();
                self.selected_email = None;
                self.sync_status = SyncStatus::Syncing;
            }
            Message::SetupError(err) => {
                if let Some(setup) = &mut self.setup {
                    setup.status = Some(err);
                }
            }
            Message::ResetAccount => {
                let email = self.account_email.clone();
                self.has_accounts = false;
                self.account_email = String::new();
                self.envelopes.clear();
                self.selected_email = None;
                self.selected_index = 0;
                self.sync_status = SyncStatus::Idle;
                self.setup = Some(SetupState {
                    phase: SetupPhase::ProviderSelect {
                        selected: ProviderOption::Gmail,
                    },
                    status: None,
                });
                return vec![Command::ResetAccount { email }];
            }

            // -- Log Viewer --
            Message::OpenLogViewer => {
                self.log_viewer = Some(LogViewerState {
                    lines: vec![],
                    scroll: 0,
                    filter_level: LogFilter::All,
                });
                return vec![Command::LoadLogs];
            }
            Message::LogViewerLoaded(lines) => {
                if let Some(lv) = &mut self.log_viewer {
                    lv.lines = lines;
                    lv.scroll = 0;
                }
            }
            Message::LogViewerCycleLevel => {
                if let Some(lv) = &mut self.log_viewer {
                    lv.filter_level = lv.filter_level.next();
                    lv.scroll = 0;
                }
            }
            Message::LogViewerScrollDown => {
                if let Some(lv) = &mut self.log_viewer {
                    let count = lv.filtered_lines().len();
                    if count > 0 {
                        lv.scroll = lv.scroll.saturating_add(1).min(
                            (count as u16).saturating_sub(1),
                        );
                    }
                }
            }
            Message::LogViewerScrollUp => {
                if let Some(lv) = &mut self.log_viewer {
                    lv.scroll = lv.scroll.saturating_sub(1);
                }
            }
            Message::CloseLogViewer => {
                self.log_viewer = None;
            }

            // -- Email actions --
            Message::DeleteSelected => {
                if let Some(uid) = self.remove_selected_envelope() {
                    return vec![Command::DeleteEmail(uid)];
                }
            }
            Message::ArchiveSelected => {
                if let Some(uid) = self.remove_selected_envelope() {
                    return vec![Command::ArchiveEmail(uid)];
                }
            }
            Message::ToggleStar => {
                if let Some(env) = self.envelopes.get_mut(self.selected_index) {
                    env.is_starred = !env.is_starred;
                    let uid = env.uid;
                    let value = env.is_starred;
                    return vec![Command::SetFlag { uid, flag: "starred".into(), value }];
                }
            }
            Message::ToggleRead => {
                if let Some(env) = self.envelopes.get_mut(self.selected_index) {
                    env.is_read = !env.is_read;
                    let uid = env.uid;
                    let value = env.is_read;
                    return vec![Command::SetFlag { uid, flag: "seen".into(), value }];
                }
            }
            Message::Refresh => {
                self.sync_status = SyncStatus::Syncing;
                return vec![Command::FetchEnvelopes];
            }

            // -- Async results --
            Message::EnvelopesFetched(envelopes) => {
                self.envelopes = envelopes;
                self.pre_search_envelopes = None;
                self.sync_status = SyncStatus::LastSync(Local::now());
                if self.selected_index >= self.envelopes.len() {
                    self.selected_index = 0;
                }
            }
            // May fire twice per open: once from cache (fast, text-only) then
            // again from IMAP (complete, with images). Overwrite is intentional.
            Message::EmailFetched(email) => {
                self.selected_email = Some(*email);
                self.preview_scroll = 0;
                self.image_protocol_cache.borrow_mut().clear();
            }
            Message::SearchResults(results) => {
                if results.is_empty() {
                    // No results — clear search mode, inbox unchanged
                    self.search_active = false;
                    self.search_query.clear();
                } else {
                    // Save originals so we can restore on search clear
                    if self.pre_search_envelopes.is_none() {
                        self.pre_search_envelopes = Some(self.envelopes.clone());
                    }
                    self.envelopes = results;
                    self.selected_index = 0;
                    self.selected_email = None;
                    self.search_active = false;
                    self.search_query.clear();
                }
            }
            Message::SyncComplete => {
                self.sync_status = SyncStatus::LastSync(Local::now());
            }
            Message::SyncError(err) => {
                self.sync_status = SyncStatus::Error(err);
            }

            // -- Animation / lifecycle --
            Message::Tick => {
                // Animation ticks handled in render loop
            }
            Message::Resize(w, h) => {
                self.terminal_size = (w, h);
            }
            Message::Quit => {
                self.running = false;
                return vec![Command::Quit];
            }
        }

        vec![Command::None]
    }

    /// Count of unread emails in the current inbox.
    pub fn unread_count(&self) -> usize {
        self.envelopes.iter().filter(|e| !e.is_read).count()
    }

    /// Returns true when no blocking overlay (setup, compose, log viewer) is active.
    fn no_overlay_active(&self) -> bool {
        self.setup.is_none() && self.compose.is_none() && self.log_viewer.is_none()
    }

    /// Compute the inbox and preview pane areas based on current terminal size.
    /// Mirrors the layout logic in `ui/mod.rs`.
    fn compute_pane_areas(&self) -> (Rect, Rect) {
        let (w, h) = self.terminal_size;
        let size = Rect::new(0, 0, w, h);

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header + search bar
                Constraint::Min(5),    // Main content
                Constraint::Length(1), // Status bar
            ])
            .split(size);

        let main_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(70),
            ])
            .split(outer[1]);

        (main_area[0], main_area[1])
    }

    /// Remove the selected envelope from the list, clamp the index, and clear
    /// the selected email. Returns the UID of the removed item, or None.
    fn remove_selected_envelope(&mut self) -> Option<u32> {
        let env = self.envelopes.get(self.selected_index)?;
        let uid = env.uid;
        self.envelopes.remove(self.selected_index);
        if self.selected_index >= self.envelopes.len() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
        self.selected_email = None;
        self.image_protocol_cache.borrow_mut().clear();
        Some(uid)
    }
}

/// Prefix a subject line with "Re:" if it does not already start with it.
fn reply_subject(original: &str) -> String {
    if original.starts_with("Re:") {
        original.to_string()
    } else {
        format!("Re: {}", original)
    }
}

/// Return a mutable reference to the currently active compose field's string.
fn active_compose_field_mut(compose: &mut ComposeState) -> &mut String {
    match compose.active_field {
        ComposeField::To => &mut compose.to,
        ComposeField::Cc => &mut compose.cc,
        ComposeField::Subject => &mut compose.subject,
        ComposeField::Body => &mut compose.body,
    }
}

/// Load the account list from config for the setup UI.
fn load_account_list() -> Vec<AccountListItem> {
    crate::config::load_config()
        .ok()
        .map(|cfg| {
            cfg.accounts
                .into_iter()
                .map(|a| AccountListItem {
                    name: a.name,
                    email: a.email,
                    provider: a.provider,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Generate dummy envelopes for Phase 1 visual development.
fn dummy_envelopes() -> Vec<Envelope> {
    let now = Local::now();
    vec![
        Envelope {
            uid: 1,
            from_name: "John Doe".into(),
            from_address: "john@example.com".into(),
            subject: "Meeting notes from today's standup".into(),
            date: now - chrono::Duration::minutes(15),
            snippet: "Hi there, here are the meeting notes from today's standup...".into(),
            is_read: false,
            is_starred: true,
            has_attachments: false,
        },
        Envelope {
            uid: 2,
            from_name: "Sarah Chen".into(),
            from_address: "sarah@company.com".into(),
            subject: "Budget review Q1 2026".into(),
            date: now - chrono::Duration::hours(2),
            snippet: "Please review the attached budget for Q1. Key changes include...".into(),
            is_read: false,
            is_starred: false,
            has_attachments: true,
        },
        Envelope {
            uid: 3,
            from_name: "Mike Johnson".into(),
            from_address: "mike@startup.io".into(),
            subject: "Lunch today?".into(),
            date: now - chrono::Duration::hours(4),
            snippet: "Hey! Want to grab lunch today? I was thinking that new place...".into(),
            is_read: true,
            is_starred: false,
            has_attachments: false,
        },
        Envelope {
            uid: 4,
            from_name: "Alice Wang".into(),
            from_address: "alice@tech.dev".into(),
            subject: "Project update: v2.0 release".into(),
            date: now - chrono::Duration::hours(8),
            snippet: "The v2.0 release is on track. Here's a summary of what's been...".into(),
            is_read: true,
            is_starred: true,
            has_attachments: true,
        },
        Envelope {
            uid: 5,
            from_name: "Bob Smith".into(),
            from_address: "bob@design.co".into(),
            subject: "Weekly report - Design team".into(),
            date: now - chrono::Duration::days(1),
            snippet: "This week the design team completed 3 major projects...".into(),
            is_read: true,
            is_starred: false,
            has_attachments: false,
        },
        Envelope {
            uid: 6,
            from_name: "GitHub".into(),
            from_address: "noreply@github.com".into(),
            subject: "[termmail] PR #42: Add IMAP backend".into(),
            date: now - chrono::Duration::days(1),
            snippet: "New pull request opened by @contributor. This PR adds the IMAP...".into(),
            is_read: true,
            is_starred: false,
            has_attachments: false,
        },
        Envelope {
            uid: 7,
            from_name: "Emma Davis".into(),
            from_address: "emma@research.org".into(),
            subject: "Paper review: TUI Design Patterns".into(),
            date: now - chrono::Duration::days(2),
            snippet: "I've completed my review of your paper. Overall it's strong but...".into(),
            is_read: true,
            is_starred: false,
            has_attachments: true,
        },
        Envelope {
            uid: 8,
            from_name: "AWS".into(),
            from_address: "no-reply@aws.amazon.com".into(),
            subject: "Your AWS bill for January 2026".into(),
            date: now - chrono::Duration::days(3),
            snippet: "Your total AWS charges for January 2026 are $47.23...".into(),
            is_read: true,
            is_starred: false,
            has_attachments: false,
        },
        Envelope {
            uid: 9,
            from_name: "Team Slack".into(),
            from_address: "notification@slack.com".into(),
            subject: "New messages in #engineering".into(),
            date: now - chrono::Duration::days(4),
            snippet: "You have 12 new messages in #engineering channel...".into(),
            is_read: true,
            is_starred: false,
            has_attachments: false,
        },
        Envelope {
            uid: 10,
            from_name: "Lisa Park".into(),
            from_address: "lisa@university.edu".into(),
            subject: "Conference talk accepted!".into(),
            date: now - chrono::Duration::days(5),
            snippet: "Congratulations! Your talk proposal for RustConf 2026 has been...".into(),
            is_read: true,
            is_starred: true,
            has_attachments: false,
        },
    ]
}

/// Generate a dummy email for Phase 1 visual development.
fn dummy_email() -> Email {
    Email {
        uid: 1,
        message_id: "<abc123@example.com>".into(),
        from_name: "John Doe".into(),
        from_address: "john@example.com".into(),
        to: vec!["user@gmail.com".into()],
        cc: vec![],
        subject: "Meeting notes from today's standup".into(),
        date: Local::now() - chrono::Duration::minutes(15),
        body_text: r#"Hi there,

Here are the meeting notes from today's standup:

1. Backend API
   - Completed the authentication module
   - Started work on the caching layer
   - ETA for completion: end of week

2. Frontend
   - Redesigned the dashboard layout
   - Fixed 3 accessibility issues
   - Performance improvements: 40% faster initial load

3. DevOps
   - Migrated CI/CD to GitHub Actions
   - Set up staging environment
   - Monitoring dashboards deployed

Action items:
- @sarah: Review the Q1 budget proposal by Wednesday
- @mike: Schedule design review for the new components
- @alice: Update the release notes for v2.0

Next standup: Tomorrow at 10:00 AM

Best regards,
John"#.into(),
        body_html: None,
        attachments: vec![],
        inline_images: HashMap::new(),
        is_read: false,
        is_starred: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an App pre-loaded with dummy data for tests, regardless of config state.
    fn test_app() -> App {
        let mut app = App::new(ratatui_image::picker::Picker::halfblocks());
        app.envelopes = dummy_envelopes();
        app.selected_email = Some(dummy_email());
        app.sync_status = SyncStatus::LastSync(Local::now());
        app
    }

    #[test]
    fn test_app_initial_state() {
        let app = test_app();
        assert!(app.running);
        assert_eq!(app.selected_index, 0);
        assert!(!app.envelopes.is_empty());
        assert!(app.selected_email.is_some());
        assert_eq!(app.active_pane, Pane::InboxList);
        assert!(!app.search_active);
        assert!(app.compose.is_none());
    }

    #[test]
    fn test_select_next() {
        let mut app = test_app();
        assert_eq!(app.selected_index, 0);
        app.update(Message::SelectNext);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_select_previous_at_zero() {
        let mut app = test_app();
        assert_eq!(app.selected_index, 0);
        app.update(Message::SelectPrevious);
        assert_eq!(app.selected_index, 0); // Should not go below 0
    }

    #[test]
    fn test_select_next_at_end() {
        let mut app = test_app();
        let last = app.envelopes.len() - 1;
        app.selected_index = last;
        app.update(Message::SelectNext);
        assert_eq!(app.selected_index, last); // Should stay at end
    }

    #[test]
    fn test_switch_pane() {
        let mut app = test_app();
        assert_eq!(app.active_pane, Pane::InboxList);
        app.update(Message::SwitchPane);
        assert_eq!(app.active_pane, Pane::EmailPreview);
        app.update(Message::SwitchPane);
        assert_eq!(app.active_pane, Pane::InboxList);
    }

    #[test]
    fn test_toggle_search() {
        let mut app = test_app();
        assert!(!app.search_active);
        app.update(Message::ToggleSearch);
        assert!(app.search_active);
        app.update(Message::ToggleSearch);
        assert!(!app.search_active);
    }

    #[test]
    fn test_search_input() {
        let mut app = test_app();
        app.update(Message::ToggleSearch);
        app.update(Message::SearchInput('h'));
        app.update(Message::SearchInput('i'));
        assert_eq!(app.search_query, "hi");
        app.update(Message::SearchBackspace);
        assert_eq!(app.search_query, "h");
    }

    #[test]
    fn test_search_clear() {
        let mut app = test_app();
        app.update(Message::ToggleSearch);
        app.update(Message::SearchInput('t'));
        app.update(Message::SearchClear);
        assert!(!app.search_active);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn test_open_compose() {
        let mut app = test_app();
        assert!(app.compose.is_none());
        app.update(Message::OpenCompose);
        assert!(app.compose.is_some());
        let compose = app.compose.as_ref().unwrap();
        assert_eq!(compose.active_field, ComposeField::To);
    }

    #[test]
    fn test_compose_cancel() {
        let mut app = test_app();
        app.update(Message::OpenCompose);
        assert!(app.compose.is_some());
        app.update(Message::ComposeCancel);
        assert!(app.compose.is_none());
    }

    #[test]
    fn test_compose_tab_fields() {
        let mut app = test_app();
        app.update(Message::OpenCompose);
        assert_eq!(app.compose.as_ref().unwrap().active_field, ComposeField::To);
        app.update(Message::ComposeTabField);
        assert_eq!(app.compose.as_ref().unwrap().active_field, ComposeField::Cc);
        app.update(Message::ComposeTabField);
        assert_eq!(app.compose.as_ref().unwrap().active_field, ComposeField::Subject);
        app.update(Message::ComposeTabField);
        assert_eq!(app.compose.as_ref().unwrap().active_field, ComposeField::Body);
        app.update(Message::ComposeTabField);
        assert_eq!(app.compose.as_ref().unwrap().active_field, ComposeField::To);
    }

    #[test]
    fn test_compose_input() {
        let mut app = test_app();
        app.update(Message::OpenCompose);
        app.update(Message::ComposeInput('a'));
        app.update(Message::ComposeInput('b'));
        assert_eq!(app.compose.as_ref().unwrap().to, "ab");
    }

    #[test]
    fn test_toggle_star() {
        let mut app = test_app();
        let was_starred = app.envelopes[0].is_starred;
        app.update(Message::ToggleStar);
        assert_eq!(app.envelopes[0].is_starred, !was_starred);
    }

    #[test]
    fn test_toggle_read() {
        let mut app = test_app();
        let was_read = app.envelopes[0].is_read;
        app.update(Message::ToggleRead);
        assert_eq!(app.envelopes[0].is_read, !was_read);
    }

    #[test]
    fn test_delete_selected() {
        let mut app = test_app();
        let original_len = app.envelopes.len();
        let uid = app.envelopes[0].uid;
        let commands = app.update(Message::DeleteSelected);
        assert_eq!(app.envelopes.len(), original_len - 1);
        assert!(app.envelopes.iter().all(|e| e.uid != uid));
        assert!(commands.iter().any(|c| matches!(c, Command::DeleteEmail(_))));
    }

    #[test]
    fn test_open_reply() {
        let mut app = test_app();
        app.update(Message::OpenReply);
        assert!(app.compose.is_some());
        let compose = app.compose.as_ref().unwrap();
        assert!(compose.subject.starts_with("Re:"));
        assert!(!compose.to.is_empty());
    }

    #[test]
    fn test_open_forward() {
        let mut app = test_app();
        app.update(Message::OpenForward);
        assert!(app.compose.is_some());
        let compose = app.compose.as_ref().unwrap();
        assert!(compose.subject.starts_with("Fwd:"));
        assert!(compose.body.contains("Forwarded message"));
    }

    #[test]
    fn test_quit() {
        let mut app = test_app();
        assert!(app.running);
        let commands = app.update(Message::Quit);
        assert!(!app.running);
        assert!(commands.iter().any(|c| matches!(c, Command::Quit)));
    }

    #[test]
    fn test_unread_count() {
        let app = test_app();
        let unread = app.unread_count();
        let expected = app.envelopes.iter().filter(|e| !e.is_read).count();
        assert_eq!(unread, expected);
    }

    #[test]
    fn test_scroll_preview() {
        let mut app = test_app();
        assert_eq!(app.preview_scroll, 0);
        app.update(Message::ScrollPreviewDown);
        assert_eq!(app.preview_scroll, 1);
        app.update(Message::ScrollPreviewDown);
        assert_eq!(app.preview_scroll, 2);
        app.update(Message::ScrollPreviewUp);
        assert_eq!(app.preview_scroll, 1);
        app.update(Message::ScrollPreviewUp);
        assert_eq!(app.preview_scroll, 0);
        app.update(Message::ScrollPreviewUp);
        assert_eq!(app.preview_scroll, 0); // Should not go below 0
    }

    #[test]
    fn test_refresh_returns_fetch_command() {
        let mut app = test_app();
        let commands = app.update(Message::Refresh);
        assert!(commands.iter().any(|c| matches!(c, Command::FetchEnvelopes)));
        assert!(matches!(app.sync_status, SyncStatus::Syncing));
    }

    #[test]
    fn test_search_submit_returns_search_command() {
        let mut app = test_app();
        app.update(Message::ToggleSearch);
        app.update(Message::SearchInput('t'));
        app.update(Message::SearchInput('e'));
        let commands = app.update(Message::SearchSubmit);
        assert!(commands.iter().any(|c| matches!(c, Command::Search(_))));
    }

    #[test]
    fn test_envelopes_fetched() {
        let mut app = test_app();
        let new_envs = vec![Envelope {
            uid: 100,
            from_name: "Test".into(),
            from_address: "test@test.com".into(),
            subject: "Test subject".into(),
            date: Local::now(),
            snippet: "Test snippet".into(),
            is_read: false,
            is_starred: false,
            has_attachments: false,
        }];
        app.update(Message::EnvelopesFetched(new_envs));
        assert_eq!(app.envelopes.len(), 1);
        assert_eq!(app.envelopes[0].uid, 100);
        assert!(matches!(app.sync_status, SyncStatus::LastSync(_)));
    }

    #[test]
    fn test_open_selected_marks_read() {
        let mut app = test_app();
        app.envelopes[0].is_read = false;
        app.update(Message::OpenSelected);
        assert!(app.envelopes[0].is_read);
        assert_eq!(app.active_pane, Pane::EmailPreview);
    }

    #[test]
    fn test_reset_account() {
        let mut app = test_app();
        app.has_accounts = true;
        app.account_email = "test@example.com".to_string();
        let commands = app.update(Message::ResetAccount);

        // State should be cleared
        assert!(!app.has_accounts);
        assert!(app.account_email.is_empty());
        assert!(app.envelopes.is_empty());
        assert!(app.selected_email.is_none());
        assert_eq!(app.selected_index, 0);
        assert!(matches!(app.sync_status, SyncStatus::Idle));

        // Setup wizard should be open at provider select
        assert!(app.setup.is_some());
        let setup = app.setup.as_ref().unwrap();
        assert!(matches!(setup.phase, SetupPhase::ProviderSelect { .. }));

        // Should return ResetAccount command with the old email
        assert!(commands
            .iter()
            .any(|c| matches!(c, Command::ResetAccount { email } if email == "test@example.com")));
    }

    #[test]
    fn test_reset_account_preserves_running() {
        let mut app = test_app();
        app.has_accounts = true;
        app.account_email = "test@example.com".to_string();
        app.update(Message::ResetAccount);

        // App should still be running (not quit)
        assert!(app.running);
    }

    #[test]
    fn test_open_log_viewer() {
        let mut app = test_app();
        assert!(app.log_viewer.is_none());
        let commands = app.update(Message::OpenLogViewer);
        assert!(app.log_viewer.is_some());
        assert!(commands.iter().any(|c| matches!(c, Command::LoadLogs)));
    }

    #[test]
    fn test_log_viewer_loaded() {
        let mut app = test_app();
        app.update(Message::OpenLogViewer);
        let lines = vec!["line1".to_string(), "line2".to_string()];
        app.update(Message::LogViewerLoaded(lines.clone()));
        let lv = app.log_viewer.as_ref().unwrap();
        assert_eq!(lv.lines, lines);
        assert_eq!(lv.scroll, 0);
    }

    #[test]
    fn test_log_viewer_scroll() {
        let mut app = test_app();
        app.update(Message::OpenLogViewer);
        let lines: Vec<String> = (0..100).map(|i| format!("line {}", i)).collect();
        app.update(Message::LogViewerLoaded(lines));

        app.update(Message::LogViewerScrollDown);
        assert_eq!(app.log_viewer.as_ref().unwrap().scroll, 1);
        app.update(Message::LogViewerScrollDown);
        assert_eq!(app.log_viewer.as_ref().unwrap().scroll, 2);
        app.update(Message::LogViewerScrollUp);
        assert_eq!(app.log_viewer.as_ref().unwrap().scroll, 1);
        app.update(Message::LogViewerScrollUp);
        assert_eq!(app.log_viewer.as_ref().unwrap().scroll, 0);
        app.update(Message::LogViewerScrollUp);
        assert_eq!(app.log_viewer.as_ref().unwrap().scroll, 0); // saturates at 0
    }

    #[test]
    fn test_close_log_viewer() {
        let mut app = test_app();
        app.update(Message::OpenLogViewer);
        assert!(app.log_viewer.is_some());
        app.update(Message::CloseLogViewer);
        assert!(app.log_viewer.is_none());
    }
}
