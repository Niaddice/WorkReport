/// Tracks the last acknowledged text while the editor owns the live draft.
pub(crate) struct JournalDraft {
    saved: String,
    saving: bool,
}

impl JournalDraft {
    pub(crate) fn new(saved: String) -> Self {
        Self {
            saved,
            saving: false,
        }
    }

    pub(crate) fn is_dirty(&self, current: &str) -> bool {
        self.saved != current
    }

    pub(crate) fn is_saving(&self) -> bool {
        self.saving
    }

    pub(crate) fn begin_save(&mut self, current: &str) -> Option<String> {
        if self.saving || !self.is_dirty(current) {
            return None;
        }
        self.saving = true;
        Some(current.to_owned())
    }

    pub(crate) fn begin_checkpoint(&mut self) {
        debug_assert!(!self.saving);
        self.saving = true;
    }

    pub(crate) fn finish_save(&mut self, persisted: Option<String>) {
        if let Some(persisted) = persisted {
            self.saved = persisted;
        }
        self.saving = false;
    }
}

#[cfg(test)]
mod tests {
    use super::JournalDraft;

    #[test]
    fn save_acknowledges_submitted_text_while_newer_edits_stay_dirty() {
        let mut draft = JournalDraft::new("原文\r\n".to_owned());
        assert_eq!(draft.begin_save("原文\r\n"), None);

        let submitted = draft.begin_save("第一段\r\n").unwrap();
        let newer = "第一段\r\n继续输入 📝\r\n";
        assert!(draft.is_saving());
        assert_eq!(draft.begin_save(newer), None);

        draft.finish_save(Some(submitted));
        assert!(!draft.is_saving());
        assert!(!draft.is_dirty("第一段\r\n"));
        assert!(draft.is_dirty(newer));
        assert_eq!(draft.begin_save(newer).as_deref(), Some(newer));
    }

    #[test]
    fn failed_save_keeps_baseline_and_leaves_text_available_to_retry() {
        let original = "磁盘原文\n";
        let edited = "本地草稿\n";
        let mut draft = JournalDraft::new(original.to_owned());
        assert_eq!(draft.begin_save(edited).as_deref(), Some(edited));

        draft.finish_save(None);
        assert!(!draft.is_saving());
        assert!(!draft.is_dirty(original));
        assert!(draft.is_dirty(edited));
        assert_eq!(draft.begin_save(edited).as_deref(), Some(edited));
    }
}
