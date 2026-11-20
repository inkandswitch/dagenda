//! The tech tree as a standalone SVG document. Every element carries hooks
//! for scripting, so a page can make it interactive without drawing it
//! again.
//!
//! | Attribute             | On                | Value                                                   |
//! |-----------------------|-------------------|---------------------------------------------------------|
//! | `data-id`             | nodes             | the task's or milestone's id                            |
//! | `data-status`         | task nodes        | `not-started`, `done`, `in-progress`, and so on         |
//! | `data-era`            | task nodes        | the milestone whose band it's in (absent on no milestone) |
//! | `data-branch`         | branches          | the branch's index, left to right                       |
//! | `data-track`          | branches, banners | the track's id                                          |
//! | `data-milestone`      | bands, eras       | the milestone's id                                      |
//! | `data-from`/`data-to` | edges             | the prerequisite's and the dependent's ids              |
//! | `class`               | nodes             | `node task` or `node milestone`; a status; `locked`, `waiting` (under way, but waiting on something), or `available`; `reached` |
//! | `class`               | branches, banners | `track`, `untracked`, or `milestones`; `hue-0` to `hue-7` for a track's colour |
//! | `class`               | eras              | `era`, and `reached`                                    |
//! | `class`               | bands             | `band reached` for a reached era; `band side` for the layer of work on no milestone |
//! | `class`               | facts             | a `facts` row of `fact` spans: `dri`, `est`, `target`, `ref`, or `link`; the last two inside an `a` to their page |
//! | `class`               | edges             | `edge`, then `met` or `waiting`, and `wanted`; or `edge member`, an empty `g` for a line the band implies (for scripts that follow the graph) |
//!
//! Colours are literal rather than CSS variables, which many non-browser
//! viewers ignore. The picture follows the system's dark mode unless the
//! page puts a `light` or `dark` class on the `svg` element.

use super::{
    BADGE_RADIUS, BranchKind, CAPTION, FACT_GAP, FACT_LINE, FONT, Fact, Hue, LINE, MARGIN, NOTCH,
    Node, PAD_Y, Point, SEPARATOR, SMALL_FONT, Shape, TITLE_FONT, TechTree,
};
use crate::{
    plan::{Plan, Progress},
    status::Status,
};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
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

impl Svg<'_> {
    fn frame(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tree = self.0;
        // The milestones' column is open ground: their lines cross it
        let panels = tree
            .branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| branch.kind != BranchKind::Milestones);
        for (index, branch) in panels {
            writeln!(
                f,
                r#"<rect class="branch {}{}" data-branch="{index}"{} x="{}" y="{}" width="{}" height="{}" rx="12"/>"#,
                kind_class(&branch.kind),
                Colour(branch.hue),
                Track(&branch.kind),
                branch.x,
                branch.y,
                branch.width,
                branch.height
            )?;
        }

        // Over the panels, so it shows on every track's colour
        for band in tree.bands.iter().filter(|band| band.milestone.is_none()) {
            writeln!(
                f,
                r#"<g class="band side"><rect x="0" y="{}" width="{}" height="{}"/><text x="{}" y="{}">Not on a milestone</text></g>"#,
                band.top,
                tree.width,
                band.bottom.saturating_sub(band.top),
                MARGIN + 6,
                band.top + 16
            )?;
        }
        for band in tree.bands.iter().filter(|band| band.reached) {
            let Some(gate) = band.milestone.and_then(|gate| tree.nodes.get(gate)) else {
                continue;
            };
            writeln!(
                f,
                r#"<rect class="band reached" data-milestone="{}" x="0" y="{}" width="{}" height="{}"/>"#,
                gate.id,
                band.top,
                tree.width,
                band.bottom.saturating_sub(band.top)
            )?;
        }

        for banner in &tree.banners {
            writeln!(
                f,
                r#"<g class="banner {}{} depth-{}"{}><rect x="{}" y="{}" width="{}" height="{}" rx="8"/><text x="{}" y="{}" text-anchor="middle">{}{}</text></g>"#,
                kind_class(&banner.kind),
                Colour(banner.hue),
                banner.depth,
                Track(&banner.kind),
                banner.x,
                banner.y,
                banner.width,
                banner.height,
                banner.x + banner.width / 2,
                banner.y + 16,
                Escaped(&banner.heading),
                Dri(banner.dri.as_ref())
            )?;
        }
        Ok(())
    }

    /// Each milestone's line, from the left edge to its gate, with its name
    /// at the left end.
    fn eras(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tree = self.0;
        for band in &tree.bands {
            let Some(gate) = band.milestone.and_then(|gate| tree.nodes.get(gate)) else {
                continue;
            };
            let reached = if band.reached { " reached" } else { "" };
            writeln!(
                f,
                r#"<g class="era{reached}" data-milestone="{}"><line class="hit" x1="{MARGIN}" y1="{y}" x2="{}" y2="{y}" style="stroke-opacity:0;stroke-width:14;stroke-dasharray:none"/><line x1="{MARGIN}" y1="{y}" x2="{}" y2="{y}"/><text x="{}" y="{}">{}</text></g>"#,
                gate.id,
                gate.x,
                gate.x,
                MARGIN + 6,
                band.top - 7,
                Escaped(&gate.lines.join(" ")),
                y = band.top,
            )?;
        }
        Ok(())
    }

    /// Each line, hopping over every line it crosses on its runs across, so
    /// a crossing never reads as a junction.
    fn edges(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tree = self.0;
        let climbs: Vec<(usize, u32, u32, u32)> = tree
            .edges
            .iter()
            .enumerate()
            .flat_map(|(line, edge)| {
                edge.route.windows(2).filter_map(move |pair| match pair {
                    [a, b] if a.x == b.x && a.y != b.y => {
                        Some((line, a.x, a.y.min(b.y), a.y.max(b.y)))
                    }
                    _ => None,
                })
            })
            .collect();
        for (line, edge) in tree.edges.iter().enumerate() {
            let (Some(from), Some(to)) = (tree.nodes.get(edge.from), tree.nodes.get(edge.to))
            else {
                continue;
            };
            let hops: Vec<Point> = edge
                .route
                .windows(2)
                .filter_map(|pair| match pair {
                    [a, b] if a.y == b.y => Some((a.y, a.x.min(b.x), a.x.max(b.x))),
                    _ => None,
                })
                .flat_map(|(y, left, right)| {
                    climbs
                        .iter()
                        .filter(move |&&(other, x, top, bottom)| {
                            other != line && left < x && x < right && top < y && y < bottom
                        })
                        .map(move |&(_, x, _, _)| Point { x, y })
                })
                .collect();
            let state = if edge.met { "met" } else { "waiting" };
            let wanted = if edge.wanted { " wanted" } else { "" };
            writeln!(
                f,
                r#"<path class="edge {state}{wanted}" data-from="{}" data-to="{}" d="{}"/>"#,
                from.id,
                to.id,
                Route {
                    points: &edge.route,
                    hops: &hops,
                }
            )?;
        }
        for edge in &tree.implied {
            let (Some(from), Some(to)) = (tree.nodes.get(edge.from), tree.nodes.get(edge.to))
            else {
                continue;
            };
            writeln!(
                f,
                r#"<g class="edge member" data-from="{}" data-to="{}"/>"#,
                from.id, to.id
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for Svg<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tree = self.0;
        let (width, height) = (tree.width, tree.height);
        writeln!(
            f,
            r#"<svg xmlns="http://www.w3.org/2000/svg" class="dagenda tech-tree" viewBox="0 0 {width} {height}" width="{width}" height="{height}" font-family="system-ui, sans-serif" font-size="{FONT}">"#
        )?;
        f.write_str(STYLE)?;
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

        self.frame(f)?;
        self.eras(f)?;
        self.edges(f)?;
        for node in &tree.nodes {
            match &node.shape {
                Shape::Task { status, locked, .. } => {
                    let era = node
                        .era
                        .and_then(|era| tree.nodes.get(era))
                        .map(|gate| &gate.id);
                    task(f, node, *status, *locked, era)?;
                }
                Shape::Milestone { progress, .. } => milestone(f, node, *progress)?,
            }
        }
        f.write_str("</svg>")
    }
}

fn task(
    f: &mut fmt::Formatter<'_>,
    node: &Node,
    status: Status,
    locked: bool,
    era: Option<&crate::id::Id>,
) -> fmt::Result {
    let class = status_class(status);
    // Work under way that waits on something (a task waiting on its own
    // parts, say) isn't dimmed: only work that hasn't started is locked
    let access = match status {
        Status::Done | Status::Dropped => "",
        Status::InProgress | Status::NeedsAttention | Status::Research if locked => " waiting",
        Status::NotStarted | Status::Deferred | Status::Idea if locked => " locked",
        Status::NotStarted
        | Status::Deferred
        | Status::Idea
        | Status::InProgress
        | Status::NeedsAttention
        | Status::Research => " available",
    };
    writeln!(
        f,
        r#"<g class="node task {class}{access}" data-id="{}" data-status="{class}"{}>"#,
        node.id,
        Era(era)
    )?;
    writeln!(
        f,
        "<title>{} ({}): {}{}{}</title>",
        Escaped(&node.lines.join(" ")),
        node.id,
        status.describe(),
        if locked { ", waiting on something" } else { "" },
        More(node)
    )?;
    writeln!(
        f,
        r#"<rect class="card" x="{}" y="{}" width="{}" height="{}" rx="10"/>"#,
        node.x, node.y, node.width, node.height
    )?;

    let (middle, bottom) = (node.middle(), node.y + node.height);
    writeln!(
        f,
        r#"<circle class="badge" cx="{middle}" cy="{bottom}" r="{BADGE_RADIUS}"/>"#
    )?;
    match status {
        Status::NotStarted => {}
        // Drawn rather than set in a font, so it looks the same everywhere
        Status::Done => writeln!(
            f,
            r#"<path class="check" d="M{},{bottom} l3,4 l6,-8"/>"#,
            middle - 5
        )?,
        Status::Deferred
        | Status::InProgress
        | Status::Idea
        | Status::NeedsAttention
        | Status::Research
        | Status::Dropped => writeln!(
            f,
            r#"<text class="mark" x="{middle}" y="{}" text-anchor="middle">{}</text>"#,
            bottom + 4,
            Escaped(&status.mark().to_string())
        )?,
    }

    label(f, node)?;
    f.write_str("</g>\n")
}

fn milestone(f: &mut fmt::Formatter<'_>, node: &Node, progress: Progress) -> fmt::Result {
    let (x, y, width, height) = (node.x, node.y, node.width, node.height);
    let reached = if progress.is_reached() {
        " reached"
    } else {
        ""
    };
    writeln!(
        f,
        r#"<g class="node milestone{reached}" data-id="{}">"#,
        node.id
    )?;
    writeln!(
        f,
        "<title>{} ({})\n{} of {} must-haves done; {} of {} tasks behind it{}</title>",
        Escaped(&node.lines.join(" ")),
        node.id,
        progress.needs.done,
        progress.needs.total,
        progress.work.done,
        progress.work.total,
        More(node)
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
    label(f, node)?;
    f.write_str("</g>\n")
}

/// A card's label lines, caption, and facts, centred.
fn label(f: &mut fmt::Formatter<'_>, node: &Node) -> fmt::Result {
    let middle = node.middle();
    let mut baseline = node.y + PAD_Y + FONT - 1;
    write!(f, r#"<text class="label" text-anchor="middle">"#)?;
    for line in &node.lines {
        write!(
            f,
            r#"<tspan x="{middle}" y="{baseline}">{}</tspan>"#,
            Escaped(line)
        )?;
        baseline += LINE;
    }
    f.write_str("</text>\n")?;

    if let Some(caption) = &node.caption {
        writeln!(
            f,
            r#"<text class="caption" x="{middle}" y="{}" text-anchor="middle" font-size="{SMALL_FONT}">{}</text>"#,
            baseline - LINE + CAPTION,
            Escaped(caption)
        )?;
    }

    let lines = u32::try_from(node.lines.len()).unwrap_or(0);
    let caption = if node.caption.is_some() { CAPTION } else { 0 };
    let mut baseline = node.y + PAD_Y + lines * LINE + caption + FACT_GAP + FACT_LINE - 3;
    for row in &node.facts {
        write!(
            f,
            r#"<text class="facts" x="{middle}" y="{baseline}" text-anchor="middle" font-size="{SMALL_FONT}">"#
        )?;
        for (index, fact) in row.iter().enumerate() {
            if index > 0 {
                write!(f, r#"<tspan class="sep">{SEPARATOR}</tspan>"#)?;
            }
            let (class, text) = (fact_class(fact), Escaped(&fact.text()).to_string());
            match fact.url() {
                Some(url) => write!(
                    f,
                    r#"<a href="{}" target="_blank"><tspan class="fact {class}">{text}</tspan></a>"#,
                    Escaped(&url)
                )?,
                None => write!(f, r#"<tspan class="fact {class}">{text}</tspan>"#)?,
            }
        }
        f.write_str("</text>\n")?;
        baseline += FACT_LINE;
    }
    Ok(())
}

const fn fact_class(fact: &Fact) -> &'static str {
    match fact {
        Fact::Dri(_) => "dri",
        Fact::Est(_) => "est",
        Fact::Target(_) => "target",
        Fact::Ref(_) => "ref",
        Fact::Link(_) => "link",
    }
}

/// A tooltip's facts and notes, after its first line.
struct More<'a>(&'a Node);

impl fmt::Display for More<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for row in &self.0.facts {
            f.write_str("\n")?;
            for (index, fact) in row.iter().enumerate() {
                if index > 0 {
                    f.write_str(SEPARATOR)?;
                }
                Escaped(&fact.text()).fmt(f)?;
            }
        }
        for note in &self.0.notes {
            write!(f, "\n\n{}", Escaped(note.as_str()))?;
        }
        Ok(())
    }
}

/// ` · @handle` after a banner's heading, for its track's DRI.
struct Dri<'a>(Option<&'a crate::handle::Handle>);

impl fmt::Display for Dri<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(dri) => write!(f, r#"<tspan class="dri">{SEPARATOR}{dri}</tspan>"#),
            None => Ok(()),
        }
    }
}

/// A route's path: straight runs between its corners, each corner rounded,
/// and straight through any point that isn't a corner. Each of `hops` on a
/// run across, clear of its corners, is a small arch over the line it
/// crosses.
struct Route<'a> {
    points: &'a [Point],
    hops: &'a [Point],
}

impl fmt::Display for Route<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const RADIUS: i64 = 8;
        const HOP: i64 = 4;

        let points: Vec<(i64, i64)> = self
            .points
            .iter()
            .map(|point| (i64::from(point.x), i64::from(point.y)))
            .collect();
        let Some(&first) = points.first() else {
            return Ok(());
        };
        write!(f, "M{},{}", first.0, first.1)?;

        let step = |from: (i64, i64), to: (i64, i64), distance: i64| {
            let (dx, dy) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
            (from.0 + dx * distance, from.1 + dy * distance)
        };
        // Each straight stretch runs from where the last corner's curve
        // ends to where the next one's begins
        let mut from = first;
        for (index, window) in points.windows(2).enumerate() {
            let &[before, corner] = window else { continue };
            let curve = points.get(index + 2).and_then(|&after| {
                let turns = (corner.0 - before.0).signum() != (after.0 - corner.0).signum()
                    || (corner.1 - before.1).signum() != (after.1 - corner.1).signum();
                let span_in = (corner.0 - before.0).abs() + (corner.1 - before.1).abs();
                let span_out = (after.0 - corner.0).abs() + (after.1 - corner.1).abs();
                let radius = RADIUS.min(span_in / 2).min(span_out / 2);
                turns.then(|| (step(corner, before, radius), step(corner, after, radius)))
            });
            let until = curve.map_or(corner, |(start, _)| start);

            if from.1 == until.1 && from.0 != until.0 {
                let direction = (until.0 - from.0).signum();
                let mut arches: Vec<i64> = self
                    .hops
                    .iter()
                    .map(|hop| (i64::from(hop.x), i64::from(hop.y)))
                    .filter(|&(x, y)| {
                        y == from.1
                            && from.0.min(until.0) + HOP < x
                            && x < from.0.max(until.0) - HOP
                    })
                    .map(|(x, _)| x)
                    .collect();
                arches.sort_unstable_by_key(|&x| x * direction);
                let sweep = u8::from(direction > 0);
                let mut reach = from.0;
                for x in arches {
                    if (x - reach) * direction < HOP {
                        continue;
                    }
                    write!(
                        f,
                        " L{},{y} A{HOP},{HOP} 0 0 {sweep} {},{y}",
                        x - direction * HOP,
                        x + direction * HOP,
                        y = from.1
                    )?;
                    reach = x + direction * HOP;
                }
            }

            write!(f, " L{},{}", until.0, until.1)?;
            from = until;
            if let Some((_, end)) = curve {
                write!(f, " Q{},{} {},{}", corner.0, corner.1, end.0, end.1)?;
                from = end;
            }
        }
        Ok(())
    }
}

/// ` data-era="id"` for a task in a milestone's band, else nothing.
struct Era<'a>(Option<&'a crate::id::Id>);

impl fmt::Display for Era<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(id) => write!(f, r#" data-era="{id}""#),
            None => Ok(()),
        }
    }
}

/// ` hue-n` for a track's colour, else nothing.
struct Colour(Option<Hue>);

impl fmt::Display for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(hue) => write!(f, " hue-{}", hue.index()),
            None => Ok(()),
        }
    }
}

/// ` data-track="id"` for a track's branch or banner, else nothing.
struct Track<'a>(&'a BranchKind);

impl fmt::Display for Track<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            BranchKind::Track(id) => write!(f, r#" data-track="{id}""#),
            BranchKind::Untracked | BranchKind::Milestones => Ok(()),
        }
    }
}

const fn kind_class(kind: &BranchKind) -> &'static str {
    match kind {
        BranchKind::Track(_) => "track",
        BranchKind::Untracked => "untracked",
        BranchKind::Milestones => "milestones",
    }
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

/// Literal colours rather than CSS variables, which many non-browser SVG
/// viewers ignore. No angle brackets in here: inline SVG styles are parsed
/// as markup.
const STYLE: &str = r"<style>
.dagenda .canvas { fill: #fff; }
.dagenda text { fill: #1d1c1a; }
.dagenda .title { font-size: 18px; font-weight: 650; }
.dagenda .branch { fill: #f4f2ee; stroke: #e4e0d8; stroke-width: 1; }
.dagenda .branch, .dagenda .banner rect { fill-opacity: 0.85; }
.dagenda .band.reached { fill: #5aa362; fill-opacity: 0.08; }
.dagenda .band.side rect { fill: #8a857c; fill-opacity: 0.08; }
.dagenda .band.side text { fill: #8a857c; font-size: 11px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; }
/* Track colours: teal, indigo, rose, ochre, sage, plum, sky, terracotta */
.dagenda .branch.hue-0, .dagenda .banner.hue-0 rect { fill: #edf6f4; stroke: #c5e4de; }
.dagenda .banner.hue-0 rect { fill: #c5e4de; }
.dagenda .banner.hue-0 text { fill: #1f5c54; }
.dagenda .branch.hue-1, .dagenda .banner.hue-1 rect { fill: #eef0fa; stroke: #cdd3f1; }
.dagenda .banner.hue-1 rect { fill: #cdd3f1; }
.dagenda .banner.hue-1 text { fill: #2f3a7a; }
.dagenda .branch.hue-2, .dagenda .banner.hue-2 rect { fill: #fbeef1; stroke: #f2c9d3; }
.dagenda .banner.hue-2 rect { fill: #f2c9d3; }
.dagenda .banner.hue-2 text { fill: #7a2a3e; }
.dagenda .branch.hue-3, .dagenda .banner.hue-3 rect { fill: #f8f2e3; stroke: #ecdcae; }
.dagenda .banner.hue-3 rect { fill: #ecdcae; }
.dagenda .banner.hue-3 text { fill: #6b5214; }
.dagenda .branch.hue-4, .dagenda .banner.hue-4 rect { fill: #eff4ea; stroke: #d3e2c4; }
.dagenda .banner.hue-4 rect { fill: #d3e2c4; }
.dagenda .banner.hue-4 text { fill: #3d5a2a; }
.dagenda .branch.hue-5, .dagenda .banner.hue-5 rect { fill: #f4eef7; stroke: #e0cdea; }
.dagenda .banner.hue-5 rect { fill: #e0cdea; }
.dagenda .banner.hue-5 text { fill: #5a2f6b; }
.dagenda .branch.hue-6, .dagenda .banner.hue-6 rect { fill: #ecf3f9; stroke: #c7dcee; }
.dagenda .banner.hue-6 rect { fill: #c7dcee; }
.dagenda .banner.hue-6 text { fill: #24506f; }
.dagenda .branch.hue-7, .dagenda .banner.hue-7 rect { fill: #f9efea; stroke: #efd0c2; }
.dagenda .banner.hue-7 rect { fill: #efd0c2; }
.dagenda .banner.hue-7 text { fill: #7a3b22; }
.dagenda .era line { stroke: #c9a227; stroke-width: 1.5; stroke-dasharray: 8 6; }
.dagenda .era text { fill: #9a7a17; font-size: 11px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; }
.dagenda .era.reached line { stroke: #5aa362; }
.dagenda .era.reached text { fill: #3f7d46; }
.dagenda .banner rect { fill: #e9e5dd; }
.dagenda .banner text { fill: #4a4640; font-size: 12px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; }
.dagenda .edge { fill: none; stroke: #a8a399; stroke-width: 2; stroke-linejoin: round; }
.dagenda .edge.met { stroke: #5aa362; }
.dagenda .edge.wanted { stroke-dasharray: 5 4; }
.dagenda .card { fill: #fff; stroke: #c9c4bb; stroke-width: 1.5; }
.dagenda .badge { fill: #fff; stroke: #8a857c; stroke-width: 1.5; }
.dagenda .mark { font-family: ui-monospace, monospace; font-size: 11px; font-weight: 700; }
.dagenda .caption { fill: #6b6760; }
.dagenda .facts { fill: #6b6760; }
.dagenda .facts .dri { fill: #4a4640; font-weight: 600; }
.dagenda .facts .sep { fill: #b4afa6; }
.dagenda .facts a .fact { fill: #2f6fd6; }
.dagenda .facts a:hover .fact { text-decoration: underline; }
.dagenda .banner .dri { text-transform: none; letter-spacing: 0; font-weight: 500; }
.dagenda .available .card { stroke: #b08a1e; stroke-width: 2; }
.dagenda .available .badge { stroke: #b08a1e; stroke-width: 2; }
.dagenda .locked { opacity: 0.55; }
.dagenda .done .card { fill: #edf6ed; stroke: #5aa362; stroke-width: 2; }
.dagenda .done .badge { fill: #5aa362; stroke: #5aa362; }
.dagenda .check { fill: none; stroke: #fff; stroke-width: 2.2; stroke-linecap: round; stroke-linejoin: round; }
.dagenda .in-progress .card { stroke: #2f6fd6; stroke-width: 2.5; }
.dagenda .in-progress .badge { stroke: #2f6fd6; }
.dagenda .in-progress .mark { fill: #2f6fd6; }
.dagenda .needs-attention .card { fill: #fdeeee; stroke: #d33; stroke-width: 2.5; }
.dagenda .needs-attention .badge { stroke: #d33; }
.dagenda .needs-attention .mark { fill: #d33; }
.dagenda .idea .card { stroke-dasharray: 5 3; }
.dagenda .research .card { stroke: #8a5cc8; stroke-dasharray: 2 3; stroke-width: 2; }
.dagenda .research .badge { stroke: #8a5cc8; }
.dagenda .research .mark { fill: #8a5cc8; }
.dagenda .deferred { opacity: 0.6; }
.dagenda .dropped { opacity: 0.45; }
.dagenda .dropped .label { text-decoration: line-through; }
.dagenda .gate { fill: #fff8e1; stroke: #c9a227; stroke-width: 2; }
.dagenda .milestone .label { font-weight: 650; }
.dagenda .reached .gate { fill: #edf6ed; stroke: #5aa362; }
@media (prefers-color-scheme: dark) {
  .dagenda:not(.light) .canvas { fill: #1a1917; }
  .dagenda:not(.light) text { fill: #ecebe8; }
  .dagenda:not(.light) .branch { fill: #232220; stroke: #302e2a; }
  .dagenda:not(.light) .branch.hue-0, .dagenda:not(.light) .banner.hue-0 rect { fill: #1c2826; stroke: #2b4a45; }
  .dagenda:not(.light) .banner.hue-0 rect { fill: #2b4a45; }
  .dagenda:not(.light) .banner.hue-0 text { fill: #a8dcd3; }
  .dagenda:not(.light) .branch.hue-1, .dagenda:not(.light) .banner.hue-1 rect { fill: #1f2130; stroke: #333b63; }
  .dagenda:not(.light) .banner.hue-1 rect { fill: #333b63; }
  .dagenda:not(.light) .banner.hue-1 text { fill: #c3cbf2; }
  .dagenda:not(.light) .branch.hue-2, .dagenda:not(.light) .banner.hue-2 rect { fill: #2b1f23; stroke: #5a2f3b; }
  .dagenda:not(.light) .banner.hue-2 rect { fill: #5a2f3b; }
  .dagenda:not(.light) .banner.hue-2 text { fill: #f2c2cf; }
  .dagenda:not(.light) .branch.hue-3, .dagenda:not(.light) .banner.hue-3 rect { fill: #29251a; stroke: #574a22; }
  .dagenda:not(.light) .banner.hue-3 rect { fill: #574a22; }
  .dagenda:not(.light) .banner.hue-3 text { fill: #ecd9a0; }
  .dagenda:not(.light) .branch.hue-4, .dagenda:not(.light) .banner.hue-4 rect { fill: #212720; stroke: #3a4d30; }
  .dagenda:not(.light) .banner.hue-4 rect { fill: #3a4d30; }
  .dagenda:not(.light) .banner.hue-4 text { fill: #c9ddb8; }
  .dagenda:not(.light) .branch.hue-5, .dagenda:not(.light) .banner.hue-5 rect { fill: #261f2a; stroke: #4b3456; }
  .dagenda:not(.light) .banner.hue-5 rect { fill: #4b3456; }
  .dagenda:not(.light) .banner.hue-5 text { fill: #dcc6e8; }
  .dagenda:not(.light) .branch.hue-6, .dagenda:not(.light) .banner.hue-6 rect { fill: #1d242b; stroke: #2c4a63; }
  .dagenda:not(.light) .banner.hue-6 rect { fill: #2c4a63; }
  .dagenda:not(.light) .banner.hue-6 text { fill: #b9d6ee; }
  .dagenda:not(.light) .branch.hue-7, .dagenda:not(.light) .banner.hue-7 rect { fill: #2a201c; stroke: #5a3626; }
  .dagenda:not(.light) .banner.hue-7 rect { fill: #5a3626; }
  .dagenda:not(.light) .banner.hue-7 text { fill: #efc4b0; }
  .dagenda:not(.light) .era line { stroke: #9a7d22; }
  .dagenda:not(.light) .era text { fill: #c9a227; }
  .dagenda:not(.light) .era.reached line { stroke: #4c8a53; }
  .dagenda:not(.light) .era.reached text { fill: #7bc283; }
  .dagenda:not(.light) .banner rect { fill: #2e2c28; }
  .dagenda:not(.light) .banner text, .dagenda:not(.light) .caption { fill: #b4afa6; }
  .dagenda:not(.light) .facts { fill: #b4afa6; }
  .dagenda:not(.light) .facts .dri { fill: #ecebe8; }
  .dagenda:not(.light) .facts .sep { fill: #6f6b63; }
  .dagenda:not(.light) .facts a .fact { fill: #7aa7f0; }
  .dagenda:not(.light) :is(.card, .badge) { fill: #1f1e1c; }
  .dagenda:not(.light) :is(.not-started, .idea, .deferred, .dropped):not(.available) :is(.card, .badge) { stroke: #5b574f; }
  .dagenda:not(.light) .done .card { fill: #1f2e20; }
  .dagenda:not(.light) .done .badge { fill: #5aa362; }
  .dagenda:not(.light) .needs-attention .card { fill: #3a1f1f; stroke: #e5615a; }
  .dagenda:not(.light) .available .card, .dagenda:not(.light) .available .badge { stroke: #d6ae3a; }
  .dagenda:not(.light) .gate { fill: #3a3218; stroke: #c9a227; }
  .dagenda:not(.light) .reached .gate { fill: #1f2e20; stroke: #5aa362; }
  .dagenda:not(.light) .edge { stroke: #6f6b63; }
  .dagenda:not(.light) .edge.met { stroke: #5aa362; }
}
.dagenda.dark .canvas { fill: #1a1917; }
.dagenda.dark text { fill: #ecebe8; }
.dagenda.dark .branch { fill: #232220; stroke: #302e2a; }
.dagenda.dark .branch.hue-0, .dagenda.dark .banner.hue-0 rect { fill: #1c2826; stroke: #2b4a45; }
.dagenda.dark .banner.hue-0 rect { fill: #2b4a45; }
.dagenda.dark .banner.hue-0 text { fill: #a8dcd3; }
.dagenda.dark .branch.hue-1, .dagenda.dark .banner.hue-1 rect { fill: #1f2130; stroke: #333b63; }
.dagenda.dark .banner.hue-1 rect { fill: #333b63; }
.dagenda.dark .banner.hue-1 text { fill: #c3cbf2; }
.dagenda.dark .branch.hue-2, .dagenda.dark .banner.hue-2 rect { fill: #2b1f23; stroke: #5a2f3b; }
.dagenda.dark .banner.hue-2 rect { fill: #5a2f3b; }
.dagenda.dark .banner.hue-2 text { fill: #f2c2cf; }
.dagenda.dark .branch.hue-3, .dagenda.dark .banner.hue-3 rect { fill: #29251a; stroke: #574a22; }
.dagenda.dark .banner.hue-3 rect { fill: #574a22; }
.dagenda.dark .banner.hue-3 text { fill: #ecd9a0; }
.dagenda.dark .branch.hue-4, .dagenda.dark .banner.hue-4 rect { fill: #212720; stroke: #3a4d30; }
.dagenda.dark .banner.hue-4 rect { fill: #3a4d30; }
.dagenda.dark .banner.hue-4 text { fill: #c9ddb8; }
.dagenda.dark .branch.hue-5, .dagenda.dark .banner.hue-5 rect { fill: #261f2a; stroke: #4b3456; }
.dagenda.dark .banner.hue-5 rect { fill: #4b3456; }
.dagenda.dark .banner.hue-5 text { fill: #dcc6e8; }
.dagenda.dark .branch.hue-6, .dagenda.dark .banner.hue-6 rect { fill: #1d242b; stroke: #2c4a63; }
.dagenda.dark .banner.hue-6 rect { fill: #2c4a63; }
.dagenda.dark .banner.hue-6 text { fill: #b9d6ee; }
.dagenda.dark .branch.hue-7, .dagenda.dark .banner.hue-7 rect { fill: #2a201c; stroke: #5a3626; }
.dagenda.dark .banner.hue-7 rect { fill: #5a3626; }
.dagenda.dark .banner.hue-7 text { fill: #efc4b0; }
.dagenda.dark .era line { stroke: #9a7d22; }
.dagenda.dark .era text { fill: #c9a227; }
.dagenda.dark .era.reached line { stroke: #4c8a53; }
.dagenda.dark .era.reached text { fill: #7bc283; }
.dagenda.dark .banner rect { fill: #2e2c28; }
.dagenda.dark .banner text, .dagenda.dark .caption { fill: #b4afa6; }
.dagenda.dark .facts { fill: #b4afa6; }
.dagenda.dark .facts .dri { fill: #ecebe8; }
.dagenda.dark .facts .sep { fill: #6f6b63; }
.dagenda.dark .facts a .fact { fill: #7aa7f0; }
.dagenda.dark :is(.card, .badge) { fill: #1f1e1c; }
.dagenda.dark :is(.not-started, .idea, .deferred, .dropped):not(.available) :is(.card, .badge) { stroke: #5b574f; }
.dagenda.dark .done .card { fill: #1f2e20; }
.dagenda.dark .done .badge { fill: #5aa362; }
.dagenda.dark .needs-attention .card { fill: #3a1f1f; stroke: #e5615a; }
.dagenda.dark .available .card, .dagenda.dark .available .badge { stroke: #d6ae3a; }
.dagenda.dark .gate { fill: #3a3218; stroke: #c9a227; }
.dagenda.dark .reached .gate { fill: #1f2e20; stroke: #5aa362; }
.dagenda.dark .edge { stroke: #6f6b63; }
.dagenda.dark .edge.met { stroke: #5aa362; }
</style>
";
