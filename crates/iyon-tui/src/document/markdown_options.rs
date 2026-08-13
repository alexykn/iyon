use pulldown_cmark::Options;

/// Explicitly selected Markdown extensions supported by [`super::MarkdownProjector`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkdownOptions {
    tables: bool,
    strikethrough: bool,
    task_lists: bool,
}

impl MarkdownOptions {
    /// Strict CommonMark parsing with all optional extensions disabled.
    pub const fn commonmark() -> Self {
        Self {
            tables: false,
            strikethrough: false,
            task_lists: false,
        }
    }

    pub const fn with_tables(mut self, enabled: bool) -> Self {
        self.tables = enabled;
        self
    }

    pub const fn with_strikethrough(mut self, enabled: bool) -> Self {
        self.strikethrough = enabled;
        self
    }

    pub const fn with_task_lists(mut self, enabled: bool) -> Self {
        self.task_lists = enabled;
        self
    }

    pub const fn tables(self) -> bool {
        self.tables
    }

    pub const fn strikethrough(self) -> bool {
        self.strikethrough
    }

    pub const fn task_lists(self) -> bool {
        self.task_lists
    }

    pub(crate) fn pulldown(self) -> Options {
        let mut options = Options::empty();
        if self.tables {
            options.insert(Options::ENABLE_TABLES);
        }
        if self.strikethrough {
            options.insert(Options::ENABLE_STRIKETHROUGH);
        }
        if self.task_lists {
            options.insert(Options::ENABLE_TASKLISTS);
        }
        options
    }
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self::commonmark()
    }
}
