//! Fish-inspired interactive line editing.
//!
//! The REPL previously used a bare `stdin.read_line`, which meant no cursor
//! movement, no history recall, and no autosuggestions — the features that
//! define the fish experience. This module supplies them.
//!
//! The design deliberately splits into two halves:
//!
//! - [`LineState`] and [`apply_key`] are a **pure state machine**. Every
//!   editing behavior — insertion, word deletion, history recall, abbreviation
//!   expansion, suggestion acceptance — is expressed as a total function over
//!   state, so all of it is unit-testable without a terminal.
//! - [`read_line`] is the thin I/O layer that puts the terminal in raw mode,
//!   translates crossterm key events into [`EditKey`], and repaints.
//!
//! When stdin is not a TTY (pipes, CI, `ae < script.ae`), [`read_line`] falls
//! back to plain buffered reads so non-interactive use is unaffected.

use crate::config::PromptConfig;
use std::collections::HashMap;

/// A key press, decoupled from crossterm so the editing logic stays testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditKey {
    /// A printable character.
    Char(char),
    /// Insert a literal space (kept distinct because it triggers abbreviation
    /// expansion, exactly as in fish).
    Space,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// Previous history entry.
    Up,
    /// Next history entry.
    Down,
    /// Accept the current ghost-text suggestion.
    AcceptSuggestion,
    /// Delete the word before the cursor (Ctrl-W).
    DeleteWord,
    /// Delete from the cursor to the start of the line (Ctrl-U).
    DeleteToStart,
    /// Delete from the cursor to the end of the line (Ctrl-K).
    DeleteToEnd,
    /// Submit the line.
    Enter,
    /// Abandon the line (Ctrl-C).
    Cancel,
    /// End of input (Ctrl-D on an empty line).
    Eof,
    /// Clear the screen (Ctrl-L).
    Clear,
}

/// What the caller should do after feeding a key to the state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditAction {
    /// Keep editing; repaint the line.
    Continue,
    /// The user submitted this line.
    Submit(String),
    /// The user abandoned the line; discard it and show a fresh prompt.
    Cancelled,
    /// End of input; the REPL should exit.
    Eof,
    /// Repaint the whole screen, then continue.
    ClearScreen,
}

/// The editable buffer plus everything needed to render ghost text.
#[derive(Debug, Clone, Default)]
pub struct LineState {
    /// Current buffer contents.
    pub buffer: String,
    /// Cursor position as a character index into `buffer`.
    pub cursor: usize,
    /// Commands available for recall and suggestion, oldest first.
    pub history: Vec<String>,
    /// Index into `history` while browsing with Up/Down; `None` means the user
    /// is editing a fresh line.
    pub history_index: Option<usize>,
    /// The line as typed before history browsing started, so Down can restore it.
    pub stashed: String,
    /// fish-style abbreviations, expanded when space is pressed.
    pub abbreviations: HashMap<String, String>,
    /// Whether ghost-text suggestions are enabled.
    pub suggestions_enabled: bool,
}

impl LineState {
    /// Build a state machine from config plus the session's history.
    pub fn new(cfg: &PromptConfig, history: Vec<String>) -> Self {
        Self {
            history,
            abbreviations: cfg.abbreviations.clone(),
            suggestions_enabled: cfg.autosuggestions,
            ..Default::default()
        }
    }

    /// Character length of the buffer.
    pub fn len(&self) -> usize {
        self.buffer.chars().count()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Byte offset corresponding to the character-indexed cursor.
    fn byte_at(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(self.buffer.len())
    }

    /// The ghost-text completion for the current buffer: the most recent
    /// history entry that starts with what has been typed.
    ///
    /// Returns only the *remainder*, which is what gets painted in dim text
    /// after the cursor. A suggestion is offered only while the cursor sits at
    /// the end of the buffer, so build state with [`Self::insert`] (or
    /// [`set_line`](Self::set_line)) rather than assigning `buffer` directly.
    ///
    /// ```
    /// use aethershell::line_editor::LineState;
    /// let mut s = LineState::default();
    /// s.suggestions_enabled = true;
    /// s.history = vec!["ls(\"./src\")".into()];
    /// s.set_line("ls(");
    /// assert_eq!(s.suggestion().as_deref(), Some("\"./src\")"));
    /// ```
    pub fn suggestion(&self) -> Option<String> {
        if !self.suggestions_enabled || self.buffer.is_empty() {
            return None;
        }
        // Only suggest when the cursor is at the end; suggesting mid-edit is
        // visually confusing and fish does not do it either.
        if self.cursor != self.len() {
            return None;
        }
        self.history
            .iter()
            .rev()
            .find(|h| h.starts_with(&self.buffer) && h.as_str() != self.buffer)
            .map(|h| h[self.buffer.len()..].to_string())
    }

    /// The word immediately before the cursor, used for abbreviation expansion.
    fn word_before_cursor(&self) -> (usize, String) {
        let upto: String = self.buffer.chars().take(self.cursor).collect();
        let start = upto
            .rfind(|c: char| c.is_whitespace() || c == '|')
            .map(|i| i + 1)
            .unwrap_or(0);
        (start, upto[start..].to_string())
    }

    /// Expand the word before the cursor if it is a registered abbreviation.
    ///
    /// Returns true when an expansion happened. Like fish, this fires on space,
    /// so what lands in history is the expanded form.
    pub fn expand_abbreviation(&mut self) -> bool {
        let (start, word) = self.word_before_cursor();
        if word.is_empty() {
            return false;
        }
        let Some(expansion) = self.abbreviations.get(&word).cloned() else {
            return false;
        };
        let start_byte = self.byte_at(start);
        let cursor_byte = self.byte_at(self.cursor);
        self.buffer
            .replace_range(start_byte..cursor_byte, &expansion);
        self.cursor = start + expansion.chars().count();
        true
    }

    /// Insert a character at the cursor.
    pub fn insert(&mut self, c: char) {
        let at = self.byte_at(self.cursor);
        self.buffer.insert(at, c);
        self.cursor += 1;
        // Typing invalidates history browsing.
        self.history_index = None;
    }

    /// Delete the character before the cursor.
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_at(self.cursor - 1);
        let end = self.byte_at(self.cursor);
        self.buffer.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Delete the character under the cursor.
    fn delete(&mut self) {
        if self.cursor >= self.len() {
            return;
        }
        let start = self.byte_at(self.cursor);
        let end = self.byte_at(self.cursor + 1);
        self.buffer.replace_range(start..end, "");
    }

    /// Delete the whitespace-delimited word before the cursor.
    fn delete_word(&mut self) {
        let upto: String = self.buffer.chars().take(self.cursor).collect();
        let trimmed = upto.trim_end();
        let start = trimmed
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);
        let start_chars = upto[..start].chars().count();
        let start_byte = self.byte_at(start_chars);
        let cursor_byte = self.byte_at(self.cursor);
        self.buffer.replace_range(start_byte..cursor_byte, "");
        self.cursor = start_chars;
    }

    /// Replace the buffer, putting the cursor at the end.
    ///
    /// Prefer this over assigning `buffer` directly: several behaviors
    /// (autosuggestions in particular) key off the cursor being at the end of
    /// the line, and a stale cursor silently disables them.
    pub fn set_line(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.cursor = s.chars().count();
        self.buffer = s;
    }

    /// Replace the buffer, putting the cursor at the end.
    fn set_buffer(&mut self, s: String) {
        self.set_line(s);
    }

    /// Step backwards through history.
    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_index {
            None => {
                self.stashed = self.buffer.clone();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_index = Some(next);
        let entry = self.history[next].clone();
        self.set_buffer(entry);
    }

    /// Step forwards through history, restoring the stashed line at the end.
    fn history_next(&mut self) {
        let Some(i) = self.history_index else {
            return;
        };
        if i + 1 >= self.history.len() {
            self.history_index = None;
            let stashed = std::mem::take(&mut self.stashed);
            self.set_buffer(stashed);
        } else {
            self.history_index = Some(i + 1);
            let entry = self.history[i + 1].clone();
            self.set_buffer(entry);
        }
    }
}

/// Feed one key to the editor and report what the caller should do.
///
/// This is the whole editing model; [`read_line`] is only responsible for
/// producing [`EditKey`]s and painting the result.
pub fn apply_key(state: &mut LineState, key: EditKey) -> EditAction {
    match key {
        EditKey::Char(c) => {
            state.insert(c);
            EditAction::Continue
        }
        EditKey::Space => {
            // fish expands abbreviations when you press space, so history and
            // the executed command both record the expanded form.
            state.expand_abbreviation();
            state.insert(' ');
            EditAction::Continue
        }
        EditKey::Backspace => {
            state.backspace();
            EditAction::Continue
        }
        EditKey::Delete => {
            state.delete();
            EditAction::Continue
        }
        EditKey::Left => {
            state.cursor = state.cursor.saturating_sub(1);
            EditAction::Continue
        }
        EditKey::Right => {
            // At the end of the line, Right accepts the suggestion — this is
            // the single most-used fish interaction.
            if state.cursor >= state.len() {
                if let Some(rest) = state.suggestion() {
                    state.buffer.push_str(&rest);
                    state.cursor = state.len();
                }
            } else {
                state.cursor += 1;
            }
            EditAction::Continue
        }
        EditKey::AcceptSuggestion => {
            if let Some(rest) = state.suggestion() {
                state.buffer.push_str(&rest);
            }
            state.cursor = state.len();
            EditAction::Continue
        }
        EditKey::Home => {
            state.cursor = 0;
            EditAction::Continue
        }
        EditKey::End => {
            state.cursor = state.len();
            EditAction::Continue
        }
        EditKey::Up => {
            state.history_prev();
            EditAction::Continue
        }
        EditKey::Down => {
            state.history_next();
            EditAction::Continue
        }
        EditKey::DeleteWord => {
            state.delete_word();
            EditAction::Continue
        }
        EditKey::DeleteToStart => {
            let at = state.byte_at(state.cursor);
            state.buffer.replace_range(..at, "");
            state.cursor = 0;
            EditAction::Continue
        }
        EditKey::DeleteToEnd => {
            let at = state.byte_at(state.cursor);
            state.buffer.truncate(at);
            EditAction::Continue
        }
        EditKey::Enter => {
            // Expand a trailing abbreviation on submit too, so `ll<Enter>`
            // behaves like `ll <space>` did.
            state.expand_abbreviation();
            EditAction::Submit(state.buffer.clone())
        }
        EditKey::Cancel => EditAction::Cancelled,
        EditKey::Eof => {
            if state.is_empty() {
                EditAction::Eof
            } else {
                EditAction::Continue
            }
        }
        EditKey::Clear => EditAction::ClearScreen,
    }
}

// ============================================================================
// TERMINAL I/O
// ============================================================================

#[cfg(feature = "native")]
mod terminal {
    use super::*;
    use crossterm::{
        cursor,
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute, queue,
        terminal::{self, ClearType},
    };
    use std::io::{IsTerminal, Write};

    /// Translate a crossterm key event into an [`EditKey`], or `None` for keys
    /// the editor does not bind.
    pub fn translate(ev: KeyEvent) -> Option<EditKey> {
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        Some(match ev.code {
            KeyCode::Char(c) if ctrl => match c {
                'c' => EditKey::Cancel,
                'd' => EditKey::Eof,
                'a' => EditKey::Home,
                'e' => EditKey::End,
                'f' => EditKey::AcceptSuggestion,
                'w' => EditKey::DeleteWord,
                'u' => EditKey::DeleteToStart,
                'k' => EditKey::DeleteToEnd,
                'l' => EditKey::Clear,
                'p' => EditKey::Up,
                'n' => EditKey::Down,
                _ => return None,
            },
            KeyCode::Char(' ') => EditKey::Space,
            KeyCode::Char(c) => EditKey::Char(c),
            KeyCode::Backspace => EditKey::Backspace,
            KeyCode::Delete => EditKey::Delete,
            KeyCode::Left => EditKey::Left,
            KeyCode::Right => EditKey::Right,
            KeyCode::Home => EditKey::Home,
            KeyCode::End => EditKey::End,
            KeyCode::Up => EditKey::Up,
            KeyCode::Down => EditKey::Down,
            KeyCode::Enter => EditKey::Enter,
            KeyCode::Esc => EditKey::Cancel,
            _ => return None,
        })
    }

    /// Repaint the prompt, buffer, ghost suggestion, and cursor.
    fn repaint(
        out: &mut impl Write,
        prompt: &str,
        state: &LineState,
        dim: &str,
        colors: bool,
    ) -> std::io::Result<()> {
        // The prompt may be multi-line (pure/two-line powerline styles); only
        // its last line shares a row with the buffer.
        let prompt_last = prompt.rsplit('\n').next().unwrap_or(prompt);
        let prompt_width = crate::prompt::visible_width(prompt_last);

        queue!(
            out,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::UntilNewLine)
        )?;
        write!(out, "{prompt_last}{}", state.buffer)?;

        if let Some(rest) = state.suggestion() {
            if colors {
                write!(out, "{dim}{rest}\x1b[0m")?;
            } else {
                // Without color the ghost text is indistinguishable from real
                // input, so it is simply not drawn.
            }
        }

        queue!(
            out,
            cursor::MoveToColumn((prompt_width + state.cursor) as u16)
        )?;
        out.flush()
    }

    /// Read one line with fish-style editing.
    ///
    /// Falls back to a plain buffered read when stdin is not a terminal, so
    /// piped input and scripts behave exactly as before.
    pub fn read_line(
        prompt: &str,
        state: &mut LineState,
        colors: bool,
    ) -> anyhow::Result<EditAction> {
        if !std::io::stdin().is_terminal() {
            return plain_read_line(prompt);
        }

        let dim = {
            let p = crate::config::get_config();
            let theme = if p.colors.theme == "custom" {
                p.colors.custom.clone()
            } else {
                crate::config::Theme::from_str(&p.colors.theme).colors()
            };
            match crate::prompt::parse_hex(&theme.dim) {
                Some((r, g, b)) => format!("\x1b[38;2;{r};{g};{b}m"),
                None => "\x1b[90m".to_string(),
            }
        };

        let mut stdout = std::io::stdout();
        terminal::enable_raw_mode()?;

        // Raw mode must be restored on *every* exit path, including errors and
        // panics in the loop body, or the user's terminal is left unusable.
        let result = (|| -> anyhow::Result<EditAction> {
            write!(stdout, "{prompt}")?;
            stdout.flush()?;
            loop {
                let Event::Key(key) = event::read()? else {
                    continue;
                };
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }
                let Some(edit) = translate(key) else {
                    continue;
                };
                match apply_key(state, edit) {
                    EditAction::Continue => {
                        repaint(&mut stdout, prompt, state, &dim, colors)?;
                    }
                    EditAction::ClearScreen => {
                        execute!(
                            stdout,
                            terminal::Clear(ClearType::All),
                            cursor::MoveTo(0, 0)
                        )?;
                        write!(stdout, "{prompt}")?;
                        repaint(&mut stdout, prompt, state, &dim, colors)?;
                    }
                    other => {
                        write!(stdout, "\r\n")?;
                        stdout.flush()?;
                        return Ok(other);
                    }
                }
            }
        })();

        terminal::disable_raw_mode()?;
        result
    }

    /// Non-interactive fallback: read a line from stdin with no editing.
    fn plain_read_line(prompt: &str) -> anyhow::Result<EditAction> {
        let mut stdout = std::io::stdout();
        write!(stdout, "{prompt}")?;
        stdout.flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Ok(EditAction::Eof);
        }
        Ok(EditAction::Submit(
            line.trim_end_matches(['\n', '\r']).to_string(),
        ))
    }
}

#[cfg(feature = "native")]
pub use terminal::read_line;

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_history(items: &[&str]) -> LineState {
        LineState {
            history: items.iter().map(|s| s.to_string()).collect(),
            suggestions_enabled: true,
            ..Default::default()
        }
    }

    fn type_str(state: &mut LineState, s: &str) {
        for c in s.chars() {
            let key = if c == ' ' {
                EditKey::Space
            } else {
                EditKey::Char(c)
            };
            apply_key(state, key);
        }
    }

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut s = LineState::default();
        type_str(&mut s, "echo");
        assert_eq!(s.buffer, "echo");
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn insert_respects_cursor_position() {
        let mut s = LineState::default();
        type_str(&mut s, "ac");
        apply_key(&mut s, EditKey::Left);
        apply_key(&mut s, EditKey::Char('b'));
        assert_eq!(s.buffer, "abc");
    }

    #[test]
    fn backspace_at_start_is_a_noop() {
        let mut s = LineState::default();
        apply_key(&mut s, EditKey::Backspace);
        assert_eq!(s.buffer, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn handles_multibyte_characters() {
        // Byte vs char indexing is the classic line-editor bug; æ is 2 bytes.
        let mut s = LineState::default();
        type_str(&mut s, "æther");
        apply_key(&mut s, EditKey::Home);
        apply_key(&mut s, EditKey::Delete);
        assert_eq!(s.buffer, "ther");
    }

    #[test]
    fn delete_word_removes_the_preceding_word() {
        let mut s = LineState::default();
        type_str(&mut s, "ls ./src ");
        apply_key(&mut s, EditKey::DeleteWord);
        assert_eq!(s.buffer, "ls ");
    }

    #[test]
    fn delete_to_start_and_end() {
        let mut s = LineState::default();
        type_str(&mut s, "abcdef");
        apply_key(&mut s, EditKey::Home);
        apply_key(&mut s, EditKey::Right);
        apply_key(&mut s, EditKey::Right);
        apply_key(&mut s, EditKey::DeleteToStart);
        assert_eq!(s.buffer, "cdef");
        apply_key(&mut s, EditKey::DeleteToEnd);
        assert_eq!(s.buffer, "");
    }

    #[test]
    fn suggests_the_most_recent_matching_history_entry() {
        let mut s = state_with_history(&["ls(\"./old\")", "ls(\"./src\")"]);
        type_str(&mut s, "ls(");
        assert_eq!(s.suggestion().as_deref(), Some("\"./src\")"));
    }

    #[test]
    fn right_arrow_accepts_the_suggestion_at_end_of_line() {
        let mut s = state_with_history(&["ls(\"./src\")"]);
        type_str(&mut s, "ls(");
        apply_key(&mut s, EditKey::Right);
        assert_eq!(s.buffer, "ls(\"./src\")");
        assert_eq!(s.cursor, s.len());
    }

    #[test]
    fn right_arrow_moves_the_cursor_mid_line() {
        let mut s = state_with_history(&["ls(\"./src\")"]);
        type_str(&mut s, "ls(");
        apply_key(&mut s, EditKey::Home);
        apply_key(&mut s, EditKey::Right);
        assert_eq!(s.buffer, "ls(", "must not have accepted a suggestion");
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn no_suggestion_when_the_cursor_is_not_at_the_end() {
        let mut s = state_with_history(&["ls(\"./src\")"]);
        type_str(&mut s, "ls(");
        apply_key(&mut s, EditKey::Left);
        assert_eq!(s.suggestion(), None);
    }

    #[test]
    fn no_suggestion_when_disabled() {
        let mut s = state_with_history(&["ls(\"./src\")"]);
        s.suggestions_enabled = false;
        type_str(&mut s, "ls(");
        assert_eq!(s.suggestion(), None);
    }

    #[test]
    fn exact_match_produces_no_suggestion() {
        let mut s = state_with_history(&["echo"]);
        type_str(&mut s, "echo");
        assert_eq!(s.suggestion(), None);
    }

    #[test]
    fn up_and_down_walk_history_and_restore_the_draft() {
        let mut s = state_with_history(&["first", "second"]);
        type_str(&mut s, "draft");
        apply_key(&mut s, EditKey::Up);
        assert_eq!(s.buffer, "second");
        apply_key(&mut s, EditKey::Up);
        assert_eq!(s.buffer, "first");
        apply_key(&mut s, EditKey::Down);
        assert_eq!(s.buffer, "second");
        apply_key(&mut s, EditKey::Down);
        assert_eq!(s.buffer, "draft", "the in-progress line should come back");
    }

    #[test]
    fn history_up_stops_at_the_oldest_entry() {
        let mut s = state_with_history(&["only"]);
        apply_key(&mut s, EditKey::Up);
        apply_key(&mut s, EditKey::Up);
        assert_eq!(s.buffer, "only");
    }

    #[test]
    fn abbreviation_expands_on_space() {
        let mut s = LineState::default();
        s.abbreviations
            .insert("ll".into(), "ls(\".\") | sort()".into());
        type_str(&mut s, "ll ");
        assert_eq!(s.buffer, "ls(\".\") | sort() ");
    }

    #[test]
    fn abbreviation_expands_on_enter() {
        let mut s = LineState::default();
        s.abbreviations.insert("gs".into(), "git.status()".into());
        type_str(&mut s, "gs");
        let action = apply_key(&mut s, EditKey::Enter);
        assert_eq!(action, EditAction::Submit("git.status()".into()));
    }

    #[test]
    fn abbreviation_expands_after_a_pipe() {
        let mut s = LineState::default();
        s.abbreviations.insert("w".into(), "where".into());
        type_str(&mut s, "ls(\".\")|w ");
        assert_eq!(s.buffer, "ls(\".\")|where ");
    }

    #[test]
    fn non_abbreviations_are_left_alone() {
        let mut s = LineState::default();
        s.abbreviations.insert("ll".into(), "ls()".into());
        type_str(&mut s, "hello ");
        assert_eq!(s.buffer, "hello ");
    }

    #[test]
    fn ctrl_d_on_empty_line_is_eof_but_otherwise_ignored() {
        let mut s = LineState::default();
        assert_eq!(apply_key(&mut s, EditKey::Eof), EditAction::Eof);
        type_str(&mut s, "x");
        assert_eq!(apply_key(&mut s, EditKey::Eof), EditAction::Continue);
    }

    #[test]
    fn cancel_discards_the_line() {
        let mut s = LineState::default();
        type_str(&mut s, "oops");
        assert_eq!(apply_key(&mut s, EditKey::Cancel), EditAction::Cancelled);
    }
}
