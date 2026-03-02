# TermMail

A terminal email client built with Rust, ratatui, and IMAP.

## Known Issues

- **Zellij breaks inline image rendering.** Zellij (terminal multiplexer) does not pass through Kitty image protocol escape sequences correctly. Test image rendering in a bare terminal (e.g. Ghostty, Kitty, or iTerm2 directly) rather than inside Zellij or tmux.

## Architecture Notes

- **Image protocol:** Uses ratatui-image with Kitty protocol (auto-detected, forced on Ghostty). Images are rendered via Unicode placeholders with diacritics.
- **Cache strategy:** SQLite cache stores text-only email bodies for instant display. A single `FetchEmail` may produce two `EmailFetched` messages: one fast from cache (text-only), then one complete from IMAP (with CID + external images). The second overwrites the first.
- **Connection pooling:** IMAP sessions are reused via acquire/return with NOOP liveness checks.
