use std::path::Path;

use anyhow::{Context, Result};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{Index, ReloadPolicy};

use crate::models::envelope::Envelope;

/// Full-text search index backed by Tantivy.
pub struct SearchIndex {
    index: Index,
    schema: Schema,
    uid_field: Field,
    from_field: Field,
    subject_field: Field,
    body_field: Field,
    snippet_field: Field,
}

impl SearchIndex {
    /// Open or create the search index at the given path.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let index_path = data_dir.join("search_index");
        std::fs::create_dir_all(&index_path)?;

        let mut schema_builder = Schema::builder();
        let uid_field = schema_builder.add_u64_field("uid", INDEXED | STORED);
        let from_field = schema_builder.add_text_field("from", TEXT | STORED);
        let subject_field = schema_builder.add_text_field("subject", TEXT | STORED);
        let body_field = schema_builder.add_text_field("body", TEXT);
        let snippet_field = schema_builder.add_text_field("snippet", TEXT | STORED);
        let schema = schema_builder.build();

        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(&index_path)?,
            schema.clone(),
        )
        .context("Failed to open or create search index")?;

        Ok(Self {
            index,
            schema,
            uid_field,
            from_field,
            subject_field,
            body_field,
            snippet_field,
        })
    }

    /// Index an envelope (lightweight metadata).
    pub fn index_envelope(&self, env: &Envelope) -> Result<()> {
        let mut writer = self
            .index
            .writer(15_000_000)
            .context("Failed to create index writer")?;

        // Delete existing doc for this UID first
        writer.delete_term(tantivy::Term::from_field_u64(self.uid_field, env.uid as u64));

        let mut doc = TantivyDocument::new();
        doc.add_u64(self.uid_field, env.uid as u64);
        doc.add_text(self.from_field, format!("{} {}", env.from_name, env.from_address));
        doc.add_text(self.subject_field, &env.subject);
        doc.add_text(self.snippet_field, &env.snippet);
        writer.add_document(doc)?;
        writer.commit()?;

        Ok(())
    }

    /// Index a full email body (called when body is fetched).
    pub fn index_body(&self, uid: u32, body_text: &str) -> Result<()> {
        let mut writer = self
            .index
            .writer(15_000_000)
            .context("Failed to create index writer")?;

        // We can't update a single field, so we need to be careful.
        // For simplicity, we just add the body text as a new document.
        // A more sophisticated approach would merge.
        let mut doc = TantivyDocument::new();
        doc.add_u64(self.uid_field, uid as u64);
        doc.add_text(self.body_field, body_text);
        writer.add_document(doc)?;
        writer.commit()?;

        Ok(())
    }

    /// Batch index multiple envelopes.
    pub fn index_envelopes(&self, envelopes: &[Envelope]) -> Result<()> {
        let mut writer = self
            .index
            .writer(15_000_000)
            .context("Failed to create index writer")?;

        for env in envelopes {
            writer.delete_term(tantivy::Term::from_field_u64(self.uid_field, env.uid as u64));

            let mut doc = TantivyDocument::new();
            doc.add_u64(self.uid_field, env.uid as u64);
            doc.add_text(self.from_field, format!("{} {}", env.from_name, env.from_address));
            doc.add_text(self.subject_field, &env.subject);
            doc.add_text(self.snippet_field, &env.snippet);
            writer.add_document(doc)?;
        }

        writer.commit()?;
        Ok(())
    }

    /// Search for envelopes matching a query string. Returns matching UIDs.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<u32>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("Failed to create index reader")?;

        let searcher = reader.searcher();
        let search_fields = vec![
            self.from_field,
            self.subject_field,
            self.body_field,
            self.snippet_field,
        ];
        let query_parser = QueryParser::for_index(&self.index, search_fields);

        let query = query_parser
            .parse_query(query)
            .context("Failed to parse search query")?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut uids = Vec::new();
        for (_score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            if let Some(uid_value) = doc.get_first(self.uid_field)
                && let Some(uid) = uid_value.as_u64()
            {
                uids.push(uid as u32);
            }
        }

        Ok(uids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::envelope::Envelope;
    use chrono::Local;

    fn test_envelope(uid: u32, subject: &str, from_name: &str, from_address: &str) -> Envelope {
        Envelope {
            uid,
            from_name: from_name.into(),
            from_address: from_address.into(),
            subject: subject.into(),
            date: Local::now(),
            snippet: String::new(),
            is_read: false,
            is_starred: false,
            has_attachments: false,
        }
    }

    #[test]
    fn test_search_index_and_query() {
        let tmp = tempfile::tempdir().unwrap();
        let idx = SearchIndex::open(tmp.path()).unwrap();

        let env1 = test_envelope(1, "Meeting tomorrow", "Alice", "alice@example.com");
        let env2 = test_envelope(2, "Invoice attached", "Bob", "bob@example.com");
        let env3 = test_envelope(3, "Lunch plans", "Alice", "alice@example.com");

        idx.index_envelope(&env1).unwrap();
        idx.index_envelope(&env2).unwrap();
        idx.index_envelope(&env3).unwrap();

        // Search by subject keyword
        let results = idx.search("invoice", 10).unwrap();
        assert_eq!(results, vec![2]);

        // Search by sender name
        let results = idx.search("Bob", 10).unwrap();
        assert_eq!(results, vec![2]);
    }

    #[test]
    fn test_search_no_results() {
        let tmp = tempfile::tempdir().unwrap();
        let idx = SearchIndex::open(tmp.path()).unwrap();

        let env = test_envelope(1, "Hello world", "Test", "test@example.com");
        idx.index_envelope(&env).unwrap();

        let results = idx.search("nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_index_body() {
        let tmp = tempfile::tempdir().unwrap();
        let idx = SearchIndex::open(tmp.path()).unwrap();

        let env = test_envelope(42, "Generic subject", "Sender", "sender@example.com");
        idx.index_envelope(&env).unwrap();
        idx.index_body(42, "unique body text for searching").unwrap();

        let results = idx.search("unique", 10).unwrap();
        assert!(results.contains(&42));
    }
}
