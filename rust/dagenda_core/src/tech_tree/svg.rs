//! The tech tree as a standalone SVG document. Every element carries hooks
//! for scripting, so a page can make it interactive without drawing it
//! again.
//!
//! | Attribute               | On          | Value                                                 |
//! |-------------------------|-------------|-------------------------------------------------------|
//! | `data-id`               | nodes       | the task's or milestone's id                          |
//! | `data-status`           | task nodes  | `not-started`, `done`, `in-progress`, and so on       |
//! | `data-lane`             | lanes       | the lane's index, top to bottom                       |
//! | `data-track`            | track lanes | the track's id                                        |
//! | `data-from`/`data-to`   | edges       | the prerequisite's and the dependent's ids            |
//! | `class`                 | nodes       | `node task` or `node milestone`, a status, `reached`  |
//! | `class`                 | edges       | `edge`, then `met` or `waiting`, and `wanted`         |
//!
//! Colours are literal rather than CSS variables, which many non-browser
//! viewers ignore. The picture follows the system's dark mode unless the
//! page puts a `light` or `dark` class on the `svg` element.

use super::{
    BOX, CAPTION, FONT, LINE, MARGIN, NOTCH, Node, PAD_X, PAD_Y, SMALL_FONT, Shape, TITLE_FONT,
    TechTree,
};
use crate::{plan::Plan, status::Status};
use alloc::string::{String, ToString};
use core::fmt;

impl TechTree {
    /// Draw as a standalone SVG document.
    ///
    /// ```
    /// let plan = dagenda_core::parse("roadmap\n[x] task parser [Parser]\n")?;
    /// let svg = dagenda_core::tech_tree::TechTree::new(&plan).to_svg();
    /// assert!(svg.starts_with("<svg"));
    /// assert!(svg.contains(r#"data-id="parser""#));
    /// # Ok::<(), dagenda_core::Error>(())
    /// ```
    #[must_use]
    pub fn to_svg(&self) -> String {
        Svg(self).to_string()
    }
}

impl Plan {
    /// Draw the plan's tech tree as a standalone SVG document.
    #[must_use]
    pub fn to_svg(&self) -> String {
        TechTree::new(self).to_svg()
    }
}

/// A tech tree, written as SVG.
struct Svg<'a>(&'a TechTree);

impl fmt::Display for Svg<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tree = self.0;
        let (width, height) = (tree.width, tree.height);
        writeln!(
            f,
            r#"<svg xmlns="http://www.w3.org/2000/svg" class="dagenda tech-tree" viewBox="0 0 {width} {height}" width="{width}" height="{height}" font-family="system-ui, sans-serif" font-size="{FONT}">"#
        )?;
        f.write_str(STYLE)?;
        f.write_str(DEFS)?;
        writeln!(
            f,
            r#"<rect class="canvas" width="{width}" height="{height}"/>"#
        )?;

        if let Some(title) = &tree.title {
            writeln!(
                f,
                r#"<text class="title" x="{MARGIN}" y="{}">{}</text>"#,
                MARGIN + TITLE_FONT,
                Escaped(title.as_str())
            )?;
        }

        for (index, lane) in tree.lanes.iter().enumerate() {
            let parity = if index % 2 == 0 { "even" } else { "odd" };
            let track = match &lane.kind {
                super::LaneKind::Track(id) => format_attr(" data-track", id.as_str()),
                super::LaneKind::Milestones | super::LaneKind::Untracked => String::new(),
            };
            let indent = u32::try_from(lane.depth).unwrap_or(0).saturating_mul(12);
            writeln!(
                f,
                r#"<g class="lane {parity}" data-lane="{index}"{track}><rect class="band" x="0" y="{}" width="{width}" height="{}"/><text class="heading" x="{}" y="{}">{}</text></g>"#,
                lane.y,
                lane.height,
                MARGIN + indent,
                lane.y + 17,
                Escaped(&lane.heading)
            )?;
        }

        for edge in &tree.edges {
            let (Some(from), Some(to)) = (tree.nodes.get(edge.from), tree.nodes.get(edge.to))
            else {
                continue;
            };
            let state = if edge.met { "met" } else { "waiting" };
            let wanted = if edge.wanted { " wanted" } else { "" };
            writeln!(
                f,
                r#"<path class="edge {state}{wanted}" data-from="{}" data-to="{}" d="{}" marker-end="url(#dagenda-arrow)"/>"#,
                from.id,
                to.id,
                Curve(from, to)
            )?;
        }

        for node in &tree.nodes {
            match &node.shape {
                Shape::Task { status, .. } => task(f, node, *status)?,
                Shape::Milestone { progress, .. } => {
                    milestone(f, node, progress.is_reached())?;
                }
            }
        }

        f.write_str("</svg>")
    }
}

fn task(f: &mut fmt::Formatter<'_>, node: &Node, status: Status) -> fmt::Result {
    let class = status_class(status);
    let (x, y) = (node.x, node.y);
    writeln!(
        f,
        r#"<g class="node task {class}" data-id="{}" data-status="{class}">"#,
        node.id
    )?;
    writeln!(
        f,
        "<title>{} ({}): {}</title>",
        Escaped(&node.lines.join(" ")),
        node.id,
        status.describe()
    )?;
    writeln!(
        f,
        r#"<rect class="card" x="{x}" y="{y}" width="{}" height="{}" rx="6"/>"#,
        node.width, node.height
    )?;

    let (box_x, box_y) = (x + PAD_X, y + PAD_Y + 3);
    writeln!(
        f,
        r#"<rect class="box" x="{box_x}" y="{box_y}" width="{BOX}" height="{BOX}" rx="2"/>"#
    )?;
    if status != Status::NotStarted {
        writeln!(
            f,
            r#"<text class="mark" x="{}" y="{}" text-anchor="middle">{}</text>"#,
            box_x + BOX / 2,
            box_y + 10,
            Escaped(&status.mark().to_string())
        )?;
    }

    label(f, node, x + PAD_X + BOX + super::BOX_GAP, "start")?;
    f.write_str("</g>\n")
}

fn milestone(f: &mut fmt::Formatter<'_>, node: &Node, reached: bool) -> fmt::Result {
    let (x, y, width, height) = (node.x, node.y, node.width, node.height);
    let reached = if reached { " reached" } else { "" };
    writeln!(
        f,
        r#"<g class="node milestone{reached}" data-id="{}">"#,
        node.id
    )?;
    writeln!(
        f,
        "<title>{} ({})</title>",
        Escaped(&node.lines.join(" ")),
        node.id
    )?;
    let middle = y + height / 2;
    writeln!(
        f,
        r#"<polygon class="gate" points="{},{y} {},{y} {},{middle} {},{} {},{} {x},{middle}"/>"#,
        x + NOTCH,
        x + width - NOTCH,
        x + width,
        x + width - NOTCH,
        y + height,
        x + NOTCH,
        y + height,
    )?;
    label(f, node, x + width / 2, "middle")?;
    f.write_str("</g>\n")
}

/// A node's label lines and caption, starting at `x`.
fn label(f: &mut fmt::Formatter<'_>, node: &Node, x: u32, anchor: &str) -> fmt::Result {
    let mut baseline = node.y + PAD_Y + FONT;
    write!(f, r#"<text class="label" text-anchor="{anchor}">"#)?;
    for line in &node.lines {
        write!(
            f,
            r#"<tspan x="{x}" y="{baseline}">{}</tspan>"#,
            Escaped(line)
        )?;
        baseline += LINE;
    }
    f.write_str("</text>\n")?;

    if let Some(caption) = &node.caption {
        writeln!(
            f,
            r#"<text class="caption" x="{x}" y="{}" text-anchor="{anchor}" font-size="{SMALL_FONT}">{}</text>"#,
            baseline - LINE + CAPTION,
            Escaped(caption)
        )?;
    }
    Ok(())
}

/// The curve from a node's right side to another's left, ending where the
/// arrowhead begins.
struct Curve<'a>(&'a Node, &'a Node);

impl fmt::Display for Curve<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (from, to) = (self.0, self.1);
        let x1 = i64::from(from.x) + i64::from(from.width);
        let y1 = i64::from(from.y) + i64::from(from.height) / 2;
        let x2 = i64::from(to.x) - 6;
        let y2 = i64::from(to.y) + i64::from(to.height) / 2;
        let bend = ((x2 - x1) / 2).max(40);
        write!(
            f,
            "M{x1},{y1} C{},{y1} {},{y2} {x2},{y2}",
            x1 + bend,
            x2 - bend
        )
    }
}

/// ` name="value"`, escaped.
fn format_attr(name: &str, value: &str) -> String {
    let mut out = String::from(name);
    out.push_str("=\"");
    out.push_str(&Escaped(value).to_string());
    out.push('"');
    out
}

const fn status_class(status: Status) -> &'static str {
    match status {
        Status::NotStarted => "not-started",
        Status::Deferred => "deferred",
        Status::Done => "done",
        Status::InProgress => "in-progress",
        Status::Idea => "idea",
        Status::NeedsAttention => "needs-attention",
        Status::Research => "research",
        Status::Dropped => "dropped",
    }
}

/// Text escaped for XML content and attribute values.
struct Escaped<'a>(&'a str);

impl fmt::Display for Escaped<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '&' => f.write_str("&amp;")?,
                '<' => f.write_str("&lt;")?,
                '>' => f.write_str("&gt;")?,
                '"' => f.write_str("&quot;")?,
                c => fmt::Write::write_char(f, c)?,
            }
        }
        Ok(())
    }
}

const DEFS: &str = r#"<defs><marker id="dagenda-arrow" viewBox="0 0 8 8" refX="1" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path class="arrow" d="M0,0 L8,4 L0,8 z"/></marker></defs>
"#;

/// Literal colours rather than CSS variables, which many non-browser SVG
/// viewers ignore. No angle brackets in here: inline SVG styles are parsed
/// as markup.
const STYLE: &str = r"<style>
.dagenda .canvas { fill: #fff; }
.dagenda text { fill: #1d1c1a; }
.dagenda .title { font-size: 18px; font-weight: 650; }
.dagenda .band { fill: #f6f5f2; }
.dagenda .odd .band { fill: #efede8; }
.dagenda .heading { fill: #6b6760; font-size: 12px; font-weight: 600; letter-spacing: 0.04em; }
.dagenda .edge { fill: none; stroke: #9a958c; stroke-width: 1.5; }
.dagenda .edge.met { stroke: #6aa56f; }
.dagenda .edge.wanted { stroke-dasharray: 5 4; }
.dagenda .arrow { fill: #9a958c; }
.dagenda .card { fill: #fff; stroke: #c9c4bb; stroke-width: 1.25; }
.dagenda .box { fill: #fff; stroke: #8a857c; stroke-width: 1.25; }
.dagenda .mark { font-family: ui-monospace, monospace; font-size: 11px; font-weight: 700; }
.dagenda .caption { fill: #6b6760; }
.dagenda .done .card { fill: #edf6ed; stroke: #6aa56f; }
.dagenda .done .box { fill: #6aa56f; stroke: #6aa56f; }
.dagenda .done .mark { fill: #fff; }
.dagenda .in-progress .card { stroke: #2f6fd6; stroke-width: 2; }
.dagenda .in-progress .mark { fill: #2f6fd6; }
.dagenda .needs-attention .card { fill: #fdeeee; stroke: #d33; stroke-width: 2; }
.dagenda .needs-attention .mark { fill: #d33; }
.dagenda .idea .card { stroke-dasharray: 5 3; }
.dagenda .research .card { stroke: #8a5cc8; stroke-dasharray: 2 3; stroke-width: 1.75; }
.dagenda .research .mark { fill: #8a5cc8; }
.dagenda .deferred { opacity: 0.6; }
.dagenda .dropped { opacity: 0.45; }
.dagenda .dropped .label { text-decoration: line-through; }
.dagenda .gate { fill: #fff8e1; stroke: #c9a227; stroke-width: 1.75; }
.dagenda .milestone .label { font-weight: 650; }
.dagenda .reached .gate { fill: #f3d36b; stroke: #a8841a; }
@media (prefers-color-scheme: dark) {
  .dagenda:not(.light) .canvas { fill: #1a1917; }
  .dagenda:not(.light) text { fill: #ecebe8; }
  .dagenda:not(.light) .band { fill: #23221f; }
  .dagenda:not(.light) .odd .band { fill: #2a2925; }
  .dagenda:not(.light) .heading, .dagenda:not(.light) .caption { fill: #9a958c; }
  .dagenda:not(.light) :is(.card, .box) { fill: #1f1e1c; stroke: #5b574f; }
  .dagenda:not(.light) .done .card { fill: #1f2e20; stroke: #6aa56f; }
  .dagenda:not(.light) .done .box { fill: #6aa56f; }
  .dagenda:not(.light) .needs-attention .card { fill: #3a1f1f; stroke: #e5615a; }
  .dagenda:not(.light) .gate { fill: #3a3218; stroke: #c9a227; }
  .dagenda:not(.light) .reached .gate { fill: #8a6d16; }
}
.dagenda.dark .canvas { fill: #1a1917; }
.dagenda.dark text { fill: #ecebe8; }
.dagenda.dark .band { fill: #23221f; }
.dagenda.dark .odd .band { fill: #2a2925; }
.dagenda.dark .heading, .dagenda.dark .caption { fill: #9a958c; }
.dagenda.dark :is(.card, .box) { fill: #1f1e1c; stroke: #5b574f; }
.dagenda.dark .done .card { fill: #1f2e20; stroke: #6aa56f; }
.dagenda.dark .done .box { fill: #6aa56f; }
.dagenda.dark .needs-attention .card { fill: #3a1f1f; stroke: #e5615a; }
.dagenda.dark .gate { fill: #3a3218; stroke: #c9a227; }
.dagenda.dark .reached .gate { fill: #8a6d16; }
</style>
";
