/// All possible state transitions in the application.
/// Every user action, timer tick, and async result maps to a Message.
#[derive(Debug, Clone)]
pub enum Message {
    // Navigation
    SelectNext,
    SelectPrevious,
    OpenSelected,
    SwitchPane,
    ScrollPreviewDown,
    ScrollPreviewUp,

    // Search
    ToggleSearch,
    SearchInput(char),
    SearchBackspace,
    SearchSubmit,
    SearchClear,

    // Compose
    OpenCompose,
    OpenReply,
    OpenReplyAll,
    OpenForward,
    ComposeInput(char),
    ComposeBackspace,
    ComposeNewline,
    ComposeTabField,
    ComposeSend,
    ComposeCancel,

    // Setup
    OpenSetup,
    SetupInput(char),
    SetupBackspace,
    SetupTabField,
    SetupSubmit,
    SetupEnter,
    SetupCancel,
    SetupComplete,
    SetupError(String),
    ResetAccount,

    // Log Viewer
    OpenLogViewer,
    LogViewerLoaded(Vec<String>),
    LogViewerScrollDown,
    LogViewerScrollUp,
    CloseLogViewer,

    // Email actions
    DeleteSelected,
    ArchiveSelected,
    ToggleStar,
    ToggleRead,
    Refresh,

    // Async results
    EnvelopesFetched(Vec<crate::models::envelope::Envelope>),
    EmailFetched(Box<crate::models::email::Email>),
    SearchResults(Vec<u32>),
    SyncComplete,
    SyncError(String),

    // Animation
    Tick,

    // App lifecycle
    Resize(u16, u16),
    Quit,
}
