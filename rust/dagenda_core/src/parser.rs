//! Hand-written parser from source text to a [`Document`].
//!
//! The notation is small and line-based, so a hand-written parser stays
//! short, adds no dependencies (keeping the Wasm small), and points every
//! error at an exact byte span.
//!
//! ```text
//! document  = "roadmap" NL { item }
//! item      = "title" text NL | track | task | milestone | extend
//! track     = "track" id [ label ] NL [ INDENT { track | task | dri | note } DEDENT ]
//! task      = box "task" id [ label ] NL [ INDENT { task | detail } DEDENT ]
//! milestone = "milestone" id [ label ] NL [ INDENT { id NL | "---" NL | target | note } DEDENT ]
//! extend    = "extend" id NL [ INDENT { task | detail } DEDENT ]
//! detail    = needs | est | target | dri | pr | issue | link | note
//! needs     = "needs" id { "," id } NL
//! est       = "est" date NL                      (target is the same)
//! date      = YYYY-MM-DD | YYYY-MM | YYYY-Qn | YYYY | EOYyy
//! link      = "link" url [ label ] NL
//! note      = "note" text NL | "note" NL INDENT markdown DEDENT
//! box       = "[" ( " " | ">" | "x" | "-" | "?" | "!" | "*" | "~" ) "]"
//! label     = "[" text "]"
//! ```
//!
//! Indentation is structure: a block's children are the lines after it that
//! are indented further, and siblings share an indent. How far is up to the
//! writer. Tabs are an error, so a tab never has to be read as some number of
//! spaces.
//!
//! Blank lines don't matter, and neither do comments: whole lines starting
//! with `//`. A long note runs until the first line indented no further than
//! its `note`, so it can hold anything, comments and code fences included.

use crate::{
    ast::{Details, Document, Extend, Item, Milestone, Task, Track, TrackChild},
    date::{Date, InvalidDate},
    handle::{Handle, InvalidHandle},
    id::{Id, InvalidId},
    reference::{GithubRef, InvalidGithubRef, InvalidUrl, Link, Ref, RefKind, Url},
    span::{Span, Spanned},
    status::Status,
    text::{EmptyNote, InvalidText, Note, Text},
};
use alloc::{string::String, vec::Vec};
use core::{fmt, iter};
use thiserror::Error;

/// Parse source text into a [`Document`].
///
/// ```
/// use dagenda_core::parser;
///
/// let document = parser::document("roadmap\ntrack core [Core]\n  [x] task parser\n")?;
/// assert_eq!(document.items.len(), 1);
/// # Ok::<(), parser::ParseError>(())
/// ```
///
/// # Errors
///
/// Returns a [`ParseError`] pointing at the first malformed line.
pub fn document(source: &str) -> Result<Document, ParseError> {
    let mut parser = Parser::new(source);
    parser.header()?;

    let mut document = Document {
        title: None,
        items: Vec::new(),
    };
    parser.children(None, |parser, line| parser.item(line, &mut document))?;
    Ok(document)
}

/// The parser's place in the source.
#[derive(Debug)]
struct Parser<'a> {
    source: &'a str,

    /// Each line's byte offset, and its text without the line break.
    lines: Vec<(usize, &'a str)>,

    /// The next line to read.
    next: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        let mut lines = Vec::new();
        let mut start = 0;
        for line in source.split('\n') {
            let text = line.strip_suffix('\r').unwrap_or(line);
            match text.strip_prefix('\u{feff}') {
                Some(text) if start == 0 => lines.push(('\u{feff}'.len_utf8(), text)),
                Some(_) | None => lines.push((start, text)),
            }
            start += line.len() + 1;
        }

        Self {
            source,
            lines,
            next: 0,
        }
    }

    /// The next line that matters, without consuming it. Blank lines and
    /// comments are skipped.
    fn peek(&mut self) -> Result<Option<Line<'a>>, ParseError> {
        while let Some(&(start, raw)) = self.lines.get(self.next) {
            if let Some(line) = Line::read(start, raw)?
                && !line.text.starts_with("//")
            {
                return Ok(Some(line));
            }
            self.next += 1;
        }
        Ok(None)
    }

    /// Consume the line [`Self::peek`] returned.
    const fn bump(&mut self) {
        self.next += 1;
    }

    fn header(&mut self) -> Result<(), ParseError> {
        let Some(line) = self.peek()? else {
            return Err(ParseError::new(
                Span::at(self.source.len()),
                ParseErrorKind::MissingHeader,
            ));
        };

        if line.text != "roadmap" {
            return Err(ParseError::new(
                line.span(),
                ParseErrorKind::UnknownHeader(line.text.into()),
            ));
        }

        self.bump();
        Ok(())
    }

    /// Read the lines indented further than `parent` (or every remaining
    /// line, at the top level), which must all share one indent, handing each
    /// to `child`.
    fn children<F: FnMut(&mut Self, Line<'a>) -> Result<(), ParseError>>(
        &mut self,
        parent: Option<usize>,
        mut child: F,
    ) -> Result<(), ParseError> {
        let mut siblings = None;
        while let Some(line) = self.peek()? {
            if parent.is_some_and(|parent| line.indent <= parent) {
                break;
            }

            match siblings {
                None => siblings = Some(line.indent),
                Some(indent) if line.indent > indent => {
                    return Err(ParseError::new(
                        line.span(),
                        ParseErrorKind::UnexpectedIndent,
                    ));
                }
                Some(indent) if line.indent < indent => {
                    return Err(ParseError::new(line.span(), ParseErrorKind::Misaligned));
                }
                Some(_) => {}
            }

            self.bump();
            child(self, line)?;
        }
        Ok(())
    }

    fn item(&mut self, line: Line<'a>, document: &mut Document) -> Result<(), ParseError> {
        match Head::read(line)? {
            Head::Title(value) => {
                if document.title.is_some() {
                    return Err(ParseError::new(line.span(), ParseErrorKind::DuplicateTitle));
                }
                if value.is_empty() {
                    return Err(ParseError::new(
                        line.span(),
                        ParseErrorKind::MissingValue("title"),
                    ));
                }
                document.title = Some(parse_value(value, Text::new)?);
            }
            Head::Track(rest) => document.items.push(Item::Track(self.track(line, rest)?)),
            Head::Task(status, rest) => {
                document
                    .items
                    .push(Item::Task(self.task(line, status, rest)?));
            }
            Head::Milestone(rest) => {
                document
                    .items
                    .push(Item::Milestone(self.milestone(line, rest)?));
            }
            Head::Extend(rest) => document.items.push(Item::Extend(self.extend(line, rest)?)),
            Head::Member(id) => {
                return Err(ParseError::new(id.span, ParseErrorKind::BareId(id.node)));
            }
            head @ (Head::Detail(..) | Head::Cut) => {
                return Err(head.misplaced(line, Place::TopLevel));
            }
        }
        Ok(())
    }

    fn track(&mut self, line: Line<'a>, rest: Piece<'a>) -> Result<Spanned<Track>, ParseError> {
        let (id, label) = id_and_label("track", rest)?;
        let mut track = Track {
            id,
            label,
            dri: None,
            notes: Vec::new(),
            children: Vec::new(),
        };

        self.children(Some(line.indent), |parser, child| {
            match Head::read(child)? {
                Head::Track(rest) => {
                    let nested = parser.track(child, rest)?;
                    track.children.push(TrackChild::Track(nested));
                }
                Head::Task(status, rest) => {
                    let task = parser.task(child, status, rest)?;
                    track.children.push(TrackChild::Task(task));
                }
                Head::Detail(key, word, value) => {
                    match parser.detail(Place::Track, child, key, word, value)? {
                        Detail::Dri(dri) => set_once(&mut track.dri, dri, key, Place::Track)?,
                        Detail::Note(note) => track.notes.push(note),
                        Detail::Needs(_)
                        | Detail::Est(_)
                        | Detail::Target(_)
                        | Detail::Ref(_)
                        | Detail::Link(_) => {
                            return Err(misplaced(key.quoted(), word.span(), Place::Track));
                        }
                    }
                }
                head @ (Head::Title(_)
                | Head::Milestone(_)
                | Head::Extend(_)
                | Head::Cut
                | Head::Member(_)) => return Err(head.misplaced(child, Place::Track)),
            }
            Ok(())
        })?;

        Ok(Spanned::new(track, line.span()))
    }

    fn task(
        &mut self,
        line: Line<'a>,
        status: Spanned<Status>,
        rest: Piece<'a>,
    ) -> Result<Spanned<Task>, ParseError> {
        let (id, label) = id_and_label("task", rest)?;
        let mut details = Details::default();
        let mut under = Vec::new();
        self.task_body(line, Place::Task, &mut details, &mut under)?;

        let task = Task {
            status,
            id,
            label,
            details,
            under,
        };
        Ok(Spanned::new(task, line.span()))
    }

    fn extend(&mut self, line: Line<'a>, rest: Piece<'a>) -> Result<Spanned<Extend>, ParseError> {
        let (id, label) = id_and_label("extend", rest)?;
        if let Some(label) = label {
            return Err(ParseError::new(label.span, ParseErrorKind::ExtendLabel));
        }

        let mut details = Details::default();
        let mut under = Vec::new();
        self.task_body(line, Place::Extend, &mut details, &mut under)?;

        let extend = Extend { id, details, under };
        Ok(Spanned::new(extend, line.span()))
    }

    /// The lines under a task or an `extend`: details and tasks.
    fn task_body(
        &mut self,
        line: Line<'a>,
        place: Place,
        details: &mut Details,
        under: &mut Vec<Spanned<Task>>,
    ) -> Result<(), ParseError> {
        self.children(Some(line.indent), |parser, child| {
            match Head::read(child)? {
                Head::Task(status, rest) => under.push(parser.task(child, status, rest)?),
                Head::Detail(key, word, value) => {
                    let detail = parser.detail(place, child, key, word, value)?;
                    add(details, detail, place)?;
                }
                head @ (Head::Title(_)
                | Head::Track(_)
                | Head::Milestone(_)
                | Head::Extend(_)
                | Head::Cut
                | Head::Member(_)) => return Err(head.misplaced(child, place)),
            }
            Ok(())
        })
    }

    fn milestone(
        &mut self,
        line: Line<'a>,
        rest: Piece<'a>,
    ) -> Result<Spanned<Milestone>, ParseError> {
        let (id, label) = id_and_label("milestone", rest)?;
        let mut milestone = Milestone {
            id,
            label,
            target: None,
            notes: Vec::new(),
            needs: Vec::new(),
            wants: Vec::new(),
            cut: None,
        };

        self.children(Some(line.indent), |parser, child| {
            match Head::read(child)? {
                Head::Member(member) => {
                    let listed = milestone.needs.iter().chain(&milestone.wants);
                    if listed.clone().any(|listed| listed.node == member.node) {
                        return Err(ParseError::new(
                            member.span,
                            ParseErrorKind::DuplicateMember(member.node),
                        ));
                    }
                    let list = if milestone.cut.is_some() {
                        &mut milestone.wants
                    } else {
                        &mut milestone.needs
                    };
                    list.push(member);
                }
                Head::Cut => {
                    if milestone.cut.is_some() {
                        return Err(ParseError::new(child.span(), ParseErrorKind::DuplicateCut));
                    }
                    milestone.cut = Some(child.span());
                }
                Head::Detail(key, word, value) => {
                    match parser.detail(Place::Milestone, child, key, word, value)? {
                        Detail::Target(target) => {
                            set_once(&mut milestone.target, target, key, Place::Milestone)?;
                        }
                        Detail::Note(note) => milestone.notes.push(note),
                        Detail::Needs(_)
                        | Detail::Est(_)
                        | Detail::Dri(_)
                        | Detail::Ref(_)
                        | Detail::Link(_) => {
                            return Err(misplaced(key.quoted(), word.span(), Place::Milestone));
                        }
                    }
                }
                head @ (Head::Title(_)
                | Head::Track(_)
                | Head::Task(..)
                | Head::Milestone(_)
                | Head::Extend(_)) => return Err(head.misplaced(child, Place::Milestone)),
            }
            Ok(())
        })?;

        Ok(Spanned::new(milestone, line.span()))
    }

    /// A `key value` line, or a `note` with its block, if `place` allows it.
    fn detail(
        &mut self,
        place: Place,
        line: Line<'a>,
        key: Key,
        word: Piece<'a>,
        value: Piece<'a>,
    ) -> Result<Detail, ParseError> {
        if !place.allows(key) {
            return Err(misplaced(key.quoted(), word.span(), place));
        }

        if value.is_empty() {
            return match key {
                Key::Note => self.note_block(line, word).map(Detail::Note),
                Key::Needs
                | Key::Est
                | Key::Target
                | Key::Dri
                | Key::Pr
                | Key::Issue
                | Key::Link => Err(ParseError::new(
                    word.span(),
                    ParseErrorKind::MissingValue(key.keyword()),
                )),
            };
        }

        Ok(match key {
            Key::Needs => Detail::Needs(ids(value)?),
            Key::Est => Detail::Est(parse_value(value, Date::new)?),
            Key::Target => Detail::Target(parse_value(value, Date::new)?),
            Key::Dri => Detail::Dri(parse_value(value, Handle::new)?),
            Key::Pr => Detail::Ref(reference(RefKind::PullRequest, value)?),
            Key::Issue => Detail::Ref(reference(RefKind::Issue, value)?),
            Key::Link => Detail::Link(link(value)?),
            Key::Note => Detail::Note(parse_value(value, Note::new)?),
        })
    }

    /// The markdown indented under a `note` line.
    fn note_block(&mut self, line: Line<'a>, word: Piece<'a>) -> Result<Spanned<Note>, ParseError> {
        let mut body: Vec<Option<Line<'a>>> = Vec::new();
        while let Some(&(start, raw)) = self.lines.get(self.next) {
            let read = Line::read(start, raw)?;
            if let Some(content) = read
                && content.indent <= line.indent
            {
                break;
            }
            if read.is_some() || !body.is_empty() {
                body.push(read);
            }
            self.next += 1;
        }

        while body.last().is_some_and(Option::is_none) {
            body.pop();
        }

        let content: Vec<Line<'a>> = body.iter().flatten().copied().collect();
        let (Some(first), Some(last)) = (content.first(), content.last()) else {
            return Err(ParseError::new(word.span(), ParseErrorKind::EmptyNoteBlock));
        };

        let common = content.iter().map(|line| line.indent).min().unwrap_or(0);
        let mut markdown = String::new();
        for (index, entry) in body.iter().enumerate() {
            if index > 0 {
                markdown.push('\n');
            }
            if let Some(content) = entry {
                markdown.extend(iter::repeat_n(' ', content.indent - common));
                markdown.push_str(content.text);
            }
        }

        let span = Span::new(first.start, last.span().end);
        let note = Note::new(&markdown).map_err(|e| ParseError::new(span, e.into()))?;
        Ok(Spanned::new(note, span))
    }
}

/// A line that isn't blank: how far it's indented, and its text.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    /// Leading spaces.
    indent: usize,

    /// The text after the indent, with trailing whitespace trimmed.
    text: &'a str,

    /// The byte offset of `text` in the source.
    start: usize,
}

impl<'a> Line<'a> {
    /// Measure the raw line starting at byte `start`: `None` if it's blank.
    fn read(start: usize, raw: &'a str) -> Result<Option<Self>, ParseError> {
        let unindented = raw.trim_start_matches(' ');
        let indent = raw.len() - unindented.len();
        let text = unindented.trim_end();
        if text.is_empty() {
            return Ok(None);
        }

        if let Some(space) = text.chars().next().filter(|c| c.is_whitespace()) {
            let at = start + indent;
            return Err(ParseError::new(
                Span::new(at, at + space.len_utf8()),
                ParseErrorKind::Tab,
            ));
        }

        Ok(Some(Self {
            indent,
            text,
            start: start + indent,
        }))
    }

    const fn span(self) -> Span {
        Span::new(self.start, self.start + self.text.len())
    }

    const fn piece(self) -> Piece<'a> {
        Piece {
            text: self.text,
            start: self.start,
        }
    }
}

/// Part of a line, and where it starts.
#[derive(Debug, Clone, Copy)]
struct Piece<'a> {
    text: &'a str,
    start: usize,
}

impl Piece<'_> {
    const fn span(self) -> Span {
        Span::new(self.start, self.start + self.text.len())
    }

    const fn is_empty(self) -> bool {
        self.text.is_empty()
    }

    /// The first word, and the rest after the whitespace that follows it.
    fn split_word(self) -> (Self, Self) {
        let end = self
            .text
            .find(char::is_whitespace)
            .unwrap_or(self.text.len());
        self.split_at(end)
    }

    /// Split at byte `at`, dropping whitespace from the start of the rest.
    fn split_at(self, at: usize) -> (Self, Self) {
        let (head, tail) = self.text.split_at_checked(at).unwrap_or((self.text, ""));
        let rest = tail.trim_start();
        let head = Self {
            text: head,
            start: self.start,
        };
        let rest = Self {
            text: rest,
            start: self.start + head.text.len() + (tail.len() - rest.len()),
        };
        (head, rest)
    }
}

/// What a line is, from how it starts.
#[derive(Debug, Clone)]
enum Head<'a> {
    Title(Piece<'a>),
    Track(Piece<'a>),
    Task(Spanned<Status>, Piece<'a>),
    Milestone(Piece<'a>),
    Extend(Piece<'a>),

    /// The key, the keyword as written, and the value after it.
    Detail(Key, Piece<'a>, Piece<'a>),

    /// `---`
    Cut,

    /// A lone id: a milestone member.
    Member(Spanned<Id>),
}

impl<'a> Head<'a> {
    fn read(line: Line<'a>) -> Result<Self, ParseError> {
        let piece = line.piece();
        if piece.text.starts_with('[') {
            return task_head(piece);
        }

        let (word, rest) = piece.split_word();
        Ok(match word.text {
            "title" => Self::Title(rest),
            "track" => Self::Track(rest),
            "milestone" => Self::Milestone(rest),
            "extend" => Self::Extend(rest),
            "task" => return Err(ParseError::new(word.span(), ParseErrorKind::MissingBox)),
            "---" if rest.is_empty() => Self::Cut,
            other => {
                if let Some(key) = Key::from_keyword(other) {
                    Self::Detail(key, word, rest)
                } else if rest.is_empty() {
                    let id = Id::new(other).map_err(|e| ParseError::new(word.span(), e.into()))?;
                    Self::Member(Spanned::new(id, word.span()))
                } else {
                    return Err(ParseError::new(
                        word.span(),
                        ParseErrorKind::UnknownLine(other.into()),
                    ));
                }
            }
        })
    }

    /// What this is, for an error about where it can't go.
    const fn what(&self) -> &'static str {
        match self {
            Self::Title(_) => "`title`",
            Self::Track(_) => "a track",
            Self::Task(..) => "a task",
            Self::Milestone(_) => "a milestone",
            Self::Extend(_) => "`extend`",
            Self::Detail(key, ..) => key.quoted(),
            Self::Cut => "a cut line (`---`)",
            Self::Member(_) => "a bare id",
        }
    }

    const fn misplaced(&self, line: Line<'_>, place: Place) -> ParseError {
        misplaced(self.what(), line.span(), place)
    }
}

/// `<box> task <rest>`
fn task_head(piece: Piece<'_>) -> Result<Head<'_>, ParseError> {
    let mut chars = piece.text.chars();
    let (Some('['), Some(mark), Some(']')) = (chars.next(), chars.next(), chars.next()) else {
        return Err(ParseError::new(
            Span::new(piece.start, piece.start + 1),
            ParseErrorKind::ExpectedBox,
        ));
    };

    let (status_box, rest) = piece.split_at(mark.len_utf8() + 2);
    let status = Status::from_mark(mark)
        .ok_or_else(|| ParseError::new(status_box.span(), ParseErrorKind::UnknownStatus(mark)))?;

    let (word, rest) = rest.split_word();
    if word.text != "task" {
        let at = if word.is_empty() {
            Span::at(status_box.span().end)
        } else {
            word.span()
        };
        return Err(ParseError::new(at, ParseErrorKind::ExpectedTask));
    }

    Ok(Head::Task(Spanned::new(status, status_box.span()), rest))
}

/// `<id> [<label>]`, after `keyword`.
fn id_and_label(
    keyword: &'static str,
    piece: Piece<'_>,
) -> Result<(Spanned<Id>, Option<Spanned<Text>>), ParseError> {
    let end = piece
        .text
        .find(|c: char| c.is_whitespace() || c == '[')
        .unwrap_or(piece.text.len());
    let (id, rest) = piece.split_at(end);
    if id.is_empty() {
        return Err(ParseError::new(
            Span::at(piece.start),
            ParseErrorKind::MissingId(keyword),
        ));
    }

    let id = parse_value(id, Id::new)?;
    let label = if rest.is_empty() {
        None
    } else {
        Some(label(rest)?)
    };
    Ok((id, label))
}

/// `[<text>]` to the end of the line, where `\` escapes the next character.
fn label(piece: Piece<'_>) -> Result<Spanned<Text>, ParseError> {
    let Some(inner) = piece.text.strip_prefix('[') else {
        return Err(ParseError::new(piece.span(), ParseErrorKind::ExpectedLabel));
    };

    let mut text = String::new();
    let mut escaped = false;
    let mut close = None;
    for (index, c) in inner.char_indices() {
        if escaped {
            text.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == ']' {
            close = Some(index);
            break;
        } else {
            text.push(c);
        }
    }

    let end = piece.start + piece.text.len();
    let Some(close) = close else {
        return Err(ParseError::new(
            Span::at(end),
            ParseErrorKind::UnclosedLabel,
        ));
    };

    let label_end = piece.start + 1 + close + 1;
    if label_end < end {
        return Err(ParseError::new(
            Span::new(label_end, end),
            ParseErrorKind::TextAfterLabel,
        ));
    }

    let span = Span::new(piece.start, label_end);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ParseError::new(span, ParseErrorKind::EmptyLabel));
    }

    let text = Text::new(trimmed).map_err(|e| ParseError::new(span, e.into()))?;
    Ok(Spanned::new(text, span))
}

/// `<id>, <id>, …` after `needs`.
fn ids(piece: Piece<'_>) -> Result<Vec<Spanned<Id>>, ParseError> {
    let mut ids = Vec::new();
    let mut rest = piece;
    loop {
        let end = rest.text.find(',').unwrap_or(rest.text.len());
        let (part, after) = rest.split_at(end);
        let part = Piece {
            text: part.text.trim_end(),
            start: part.start,
        };

        if part.is_empty() {
            return Err(ParseError::new(
                Span::at(part.start),
                ParseErrorKind::EmptyNeed,
            ));
        }
        if part.text.contains(char::is_whitespace) {
            return Err(ParseError::new(part.span(), ParseErrorKind::NeedsSeparator));
        }
        ids.push(parse_value(part, Id::new)?);

        let Some(next) = after.text.strip_prefix(',') else {
            break;
        };
        let trimmed = next.trim_start();
        rest = Piece {
            text: trimmed,
            start: after.start + 1 + (next.len() - trimmed.len()),
        };
    }
    Ok(ids)
}

/// `<url> [<label>]` after `link`.
fn link(piece: Piece<'_>) -> Result<Spanned<Link>, ParseError> {
    let (address, rest) = piece.split_word();
    let url = parse_value(address, Url::new)?;
    let label = if rest.is_empty() {
        None
    } else {
        Some(label(rest)?)
    };
    let end = label.as_ref().map_or(url.span.end, |label| label.span.end);
    let link = Link {
        url: url.node,
        label: label.map(|label| label.node),
    };
    Ok(Spanned::new(link, Span::new(piece.start, end)))
}

fn reference(kind: RefKind, piece: Piece<'_>) -> Result<Spanned<Ref>, ParseError> {
    parse_value(piece, |written| {
        GithubRef::new(written).map(|target| Ref { kind, target })
    })
}

/// Parse `piece` with `parse`, pointing any error at it.
fn parse_value<T, E: Into<ParseErrorKind>, F: FnOnce(&str) -> Result<T, E>>(
    piece: Piece<'_>,
    parse: F,
) -> Result<Spanned<T>, ParseError> {
    parse(piece.text)
        .map(|node| Spanned::new(node, piece.span()))
        .map_err(|e| ParseError::new(piece.span(), e.into()))
}

/// A detail line, parsed.
#[derive(Debug)]
enum Detail {
    Needs(Vec<Spanned<Id>>),
    Est(Spanned<Date>),
    Target(Spanned<Date>),
    Dri(Spanned<Handle>),
    Ref(Spanned<Ref>),
    Link(Spanned<Link>),
    Note(Spanned<Note>),
}

/// The keyword that starts a detail line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Needs,
    Est,
    Target,
    Dri,
    Pr,
    Issue,
    Link,
    Note,
}

impl Key {
    fn from_keyword(word: &str) -> Option<Self> {
        match word {
            "needs" => Some(Self::Needs),
            "est" => Some(Self::Est),
            "target" => Some(Self::Target),
            "dri" => Some(Self::Dri),
            "pr" => Some(Self::Pr),
            "issue" => Some(Self::Issue),
            "link" => Some(Self::Link),
            "note" => Some(Self::Note),
            _ => None,
        }
    }

    const fn keyword(self) -> &'static str {
        match self {
            Self::Needs => "needs",
            Self::Est => "est",
            Self::Target => "target",
            Self::Dri => "dri",
            Self::Pr => "pr",
            Self::Issue => "issue",
            Self::Link => "link",
            Self::Note => "note",
        }
    }

    const fn quoted(self) -> &'static str {
        match self {
            Self::Needs => "`needs`",
            Self::Est => "`est`",
            Self::Target => "`target`",
            Self::Dri => "`dri`",
            Self::Pr => "`pr`",
            Self::Issue => "`issue`",
            Self::Link => "`link`",
            Self::Note => "`note`",
        }
    }
}

/// Add a parsed detail to a task's or an `extend`'s details.
fn add(details: &mut Details, detail: Detail, place: Place) -> Result<(), ParseError> {
    match detail {
        Detail::Needs(ids) => details.needs.extend(ids),
        Detail::Est(est) => set_once(&mut details.est, est, Key::Est, place)?,
        Detail::Target(target) => set_once(&mut details.target, target, Key::Target, place)?,
        Detail::Dri(dri) => set_once(&mut details.dri, dri, Key::Dri, place)?,
        Detail::Ref(reference) => details.refs.push(reference),
        Detail::Link(link) => details.links.push(link),
        Detail::Note(note) => details.notes.push(note),
    }
    Ok(())
}

/// Fill a detail that a block can only have one of.
fn set_once<T>(
    slot: &mut Option<Spanned<T>>,
    value: Spanned<T>,
    key: Key,
    place: Place,
) -> Result<(), ParseError> {
    if slot.is_some() {
        return Err(ParseError::new(
            value.span,
            ParseErrorKind::DuplicateDetail {
                key: key.keyword(),
                place,
            },
        ));
    }
    *slot = Some(value);
    Ok(())
}

const fn misplaced(what: &'static str, span: Span, place: Place) -> ParseError {
    ParseError::new(span, ParseErrorKind::Misplaced { what, place })
}

/// Where a line is: at the top level, or in a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    /// Not indented under anything.
    TopLevel,

    /// Under a `track` line.
    Track,

    /// Under a `task` line.
    Task,

    /// Under a `milestone` line.
    Milestone,

    /// Under an `extend` line.
    Extend,
}

impl Place {
    /// The place as a noun, as in "a task can only have one `est`".
    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::TopLevel => "the top level",
            Self::Track => "a track",
            Self::Task => "a task",
            Self::Milestone => "a milestone",
            Self::Extend => "an `extend`",
        }
    }

    /// Whether a detail line with `key` can go here.
    const fn allows(self, key: Key) -> bool {
        match self {
            Self::Task | Self::Extend => true,
            Self::Track => matches!(key, Key::Dri | Key::Note),
            Self::Milestone => matches!(key, Key::Target | Key::Note),
            Self::TopLevel => false,
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TopLevel => f.write_str("at the top level"),
            Self::Track | Self::Task | Self::Milestone | Self::Extend => {
                write!(f, "in {}", self.noun())
            }
        }
    }
}

/// Malformed source text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind}")]
pub struct ParseError {
    span: Span,
    kind: ParseErrorKind,
}

impl ParseError {
    /// An error at `span`.
    #[must_use]
    pub const fn new(span: Span, kind: ParseErrorKind) -> Self {
        Self { span, kind }
    }

    /// Where in the source the problem is.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// What the problem is.
    #[must_use]
    pub const fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }
}

/// The ways source text can be malformed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseErrorKind {
    /// Nothing but blank lines and comments.
    #[error("missing header; start with `roadmap`")]
    MissingHeader,

    /// The first line isn't `roadmap`.
    #[error("expected `roadmap` on the first line, not {0:?}")]
    UnknownHeader(String),

    /// Indentation that isn't spaces.
    #[error("indent with spaces, not tabs")]
    Tab,

    /// Indented further than its siblings, under a line that can't hold
    /// anything.
    #[error("this line is indented further than the one above it, which can't hold anything")]
    UnexpectedIndent,

    /// Indented less than its siblings, but more than the block they're in.
    #[error("this line's indent doesn't line up with the lines above it")]
    Misaligned,

    /// `task` without a status box.
    #[error("a task starts with a status box, as in `[ ] task …`")]
    MissingBox,

    /// A `[` that doesn't open a three-character box.
    #[error("expected a status box, like `[ ]`")]
    ExpectedBox,

    /// A box with an unknown mark in it.
    #[error(
        "unknown status box `[{0}]`; use `[ ]`, `[>]`, `[x]`, `[-]`, `[?]`, `[!]`, `[*]`, or `[~]`"
    )]
    UnknownStatus(char),

    /// A status box followed by something other than `task`.
    #[error("expected `task` after the status box")]
    ExpectedTask,

    /// A block keyword with no id after it.
    #[error("expected an id after `{0}`")]
    MissingId(&'static str),

    /// An id that isn't valid.
    #[error(transparent)]
    Id(#[from] InvalidId),

    /// Something after the id that isn't a label.
    #[error("expected a label in brackets, or the end of the line")]
    ExpectedLabel,

    /// A label with no closing `]`.
    #[error("expected `]` to close the label")]
    UnclosedLabel,

    /// Text after a label's closing `]`.
    #[error("unexpected text after the label")]
    TextAfterLabel,

    /// `[]`, or brackets holding only whitespace.
    #[error("empty label; leave out the brackets to show the id instead")]
    EmptyLabel,

    /// A label on an `extend` line.
    #[error("`extend` takes no label: the task it extends already has one")]
    ExtendLabel,

    /// A label, title, or value that isn't valid text.
    #[error(transparent)]
    Text(#[from] InvalidText),

    /// A `dri` that isn't a valid handle.
    #[error(transparent)]
    Handle(#[from] InvalidHandle),

    /// A `pr` or `issue` that isn't a valid reference.
    #[error(transparent)]
    GithubRef(#[from] InvalidGithubRef),

    /// An `est` or `target` that isn't a date.
    #[error(transparent)]
    Date(#[from] InvalidDate),

    /// A `link` that isn't a web address.
    #[error(transparent)]
    Url(#[from] InvalidUrl),

    /// A one-line note that's empty.
    #[error(transparent)]
    Note(#[from] EmptyNote),

    /// A `note` line with nothing after it and nothing indented under it.
    #[error("write the note after `note`, or indent it on the lines below")]
    EmptyNoteBlock,

    /// A detail keyword with nothing after it.
    #[error("expected a value after `{0}`")]
    MissingValue(&'static str),

    /// `needs a, , b`, or a trailing comma.
    #[error("expected an id between the commas")]
    EmptyNeed,

    /// `needs a b`
    #[error("separate the ids with commas, as in `needs a, b`")]
    NeedsSeparator,

    /// A second `title` line.
    #[error("a document can only have one title")]
    DuplicateTitle,

    /// A second line for a detail a block can only have one of.
    #[error("{} can only have one `{key}`", .place.noun())]
    DuplicateDetail {
        /// The detail's keyword.
        key: &'static str,

        /// The block it's in.
        place: Place,
    },

    /// The same member twice in one milestone.
    #[error("`{0}` is already listed in this milestone")]
    DuplicateMember(Id),

    /// A second `---` in one milestone.
    #[error("a milestone has only one cut line")]
    DuplicateCut,

    /// A line in a place the notation doesn't allow it.
    #[error("{what} can't go {place}")]
    Misplaced {
        /// What the line is, as in "`needs`" or "a track".
        what: &'static str,

        /// Where it is.
        place: Place,
    },

    /// A lone id at the top level, which usually means a missing `milestone`.
    #[error("`{0}` on its own means nothing here; did you mean `milestone {0}`?")]
    BareId(Id),

    /// A line that starts with an unknown word.
    #[error(
        "unknown line starting `{0}`; expected `track`, `[ ] task`, `milestone`, `extend`, or a detail like `needs`"
    )]
    UnknownLine(String),
}
