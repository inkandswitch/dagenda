//! Text as written: titles, labels, values, and notes.

use alloc::string::String;
use core::fmt;
use thiserror::Error;

/// One line of text: a title, a label, or a value like `EOY26`.
///
/// ```
/// use dagenda_core::text::{InvalidText, Text};
///
/// assert_eq!(Text::new("New repo architecture")?.as_str(), "New repo architecture");
/// assert_eq!(Text::new(" padded"), Err(InvalidText::Untrimmed));
/// # Ok::<(), InvalidText>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Text(String);

impl Text {
    /// Check and wrap one line of text.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidText`] if the text is empty, has whitespace around
    /// it, or contains a control character such as a tab or a line break.
    pub fn new(text: &str) -> Result<Self, InvalidText> {
        if text.is_empty() {
            return Err(InvalidText::Empty);
        }

        if text.trim() != text {
            return Err(InvalidText::Untrimmed);
        }

        if let Some(c) = text.chars().find(|c| c.is_control()) {
            return Err(InvalidText::Control(c));
        }

        Ok(Self(text.into()))
    }

    /// The text as written.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Markdown: the text after `note`, or the block indented under it.
///
/// ```
/// use dagenda_core::text::Note;
///
/// let note = Note::new("Fans in events from **Subduction**.")?;
/// assert!(note.as_str().contains("Subduction"));
/// # Ok::<(), dagenda_core::text::EmptyNote>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note(String);

impl Note {
    /// Wrap some markdown.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyNote`] if the markdown is only whitespace.
    pub fn new(markdown: &str) -> Result<Self, EmptyNote> {
        if markdown.trim().is_empty() {
            return Err(EmptyNote);
        }

        Ok(Self(markdown.into()))
    }

    /// The markdown, with the block's common indent removed.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a string isn't valid [`Text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidText {
    /// Nothing there.
    #[error("expected some text")]
    Empty,

    /// Whitespace at the start or the end.
    #[error("the text has whitespace around it")]
    Untrimmed,

    /// A control character, such as a tab.
    #[error("the text contains the control character {0:?}")]
    Control(char),
}

/// A note with nothing in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a note can't be empty")]
pub struct EmptyNote;
