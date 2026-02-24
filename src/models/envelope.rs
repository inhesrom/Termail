use chrono::{DateTime, Datelike, Local};

/// Lightweight email metadata for inbox list display.
/// Kept small so we can hold thousands in memory cheaply.
#[derive(Debug, Clone)]
pub struct Envelope {
    pub uid: u32,
    pub from_name: String,
    pub from_address: String,
    pub subject: String,
    pub date: DateTime<Local>,
    pub snippet: String,
    pub is_read: bool,
    pub is_starred: bool,
    pub has_attachments: bool,
}

impl Envelope {
    /// Format the date for display in the inbox list.
    /// Shows time if today, otherwise shows date.
    pub fn display_date(&self) -> String {
        let now = Local::now();
        if self.date.date_naive() == now.date_naive() {
            self.date.format("%l:%M %p").to_string().trim().to_string()
        } else if self.date.year() == now.year() {
            self.date.format("%b %d").to_string()
        } else {
            self.date.format("%b %d, %Y").to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_envelope(date: DateTime<Local>) -> Envelope {
        Envelope {
            uid: 1,
            from_name: "Test".into(),
            from_address: "test@test.com".into(),
            subject: "Test".into(),
            date,
            snippet: "".into(),
            is_read: false,
            is_starred: false,
            has_attachments: false,
        }
    }

    #[test]
    fn test_display_date_today() {
        let env = make_envelope(Local::now());
        let display = env.display_date();
        // Should show time format like "10:30 AM"
        assert!(display.contains("AM") || display.contains("PM"));
    }

    #[test]
    fn test_display_date_this_year() {
        let date = Local::now() - chrono::Duration::days(30);
        // Only test if still same year
        if date.year() == Local::now().year() {
            let env = make_envelope(date);
            let display = env.display_date();
            // Should show "Mon DD" format
            assert!(!display.contains("AM") && !display.contains("PM"));
        }
    }

    #[test]
    fn test_display_date_different_year() {
        let date = Local::now() - chrono::Duration::days(400);
        let env = make_envelope(date);
        let display = env.display_date();
        // Should include year
        assert!(display.contains(&(Local::now().year() - 1).to_string())
            || display.contains(&(Local::now().year() - 2).to_string()));
    }
}
