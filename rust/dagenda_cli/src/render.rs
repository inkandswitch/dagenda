//! `dagenda render`: a plan as text, or its tech tree as SVG.

use clap::ValueEnum;
use dagenda_core::plan::Plan;
use std::path::Path;

/// What to render a plan as.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, ValueEnum)]
pub enum Format {
    /// Milestones and tracks, as text
    #[default]
    Text,

    /// The tech tree, as SVG
    Svg,
}

impl Format {
    /// The format a path's extension asks for, in any case: `.svg` for SVG,
    /// `.txt` for text. Anything else asks for nothing.
    ///
    /// ```
    /// use dagenda_cli::render::Format;
    /// use std::path::Path;
    ///
    /// assert_eq!(Format::for_path(Path::new("plan.svg")), Some(Format::Svg));
    /// assert_eq!(Format::for_path(Path::new("PLAN.SVG")), Some(Format::Svg));
    /// assert_eq!(Format::for_path(Path::new("plan.md")), None);
    /// assert_eq!(Format::for_path(Path::new("plan.dagenda")), None);
    /// ```
    #[must_use]
    pub fn for_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("svg") {
            Some(Self::Svg)
        } else if extension.eq_ignore_ascii_case("txt") {
            Some(Self::Text)
        } else {
            None
        }
    }

    /// Render `plan` in this format, ending with a newline.
    #[must_use]
    pub fn render(self, plan: &Plan) -> String {
        let mut rendered = match self {
            Self::Text => plan.to_text(),
            Self::Svg => plan.to_svg(),
        };
        if !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        rendered
    }
}
