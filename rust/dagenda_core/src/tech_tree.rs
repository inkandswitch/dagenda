//! The tech tree: tasks and milestones laid out bottom to top by what they
//! need, as in a game's tech tree. Time goes up.
//!
//! Each row is a tier: one more than the deepest thing in it waits on, so
//! the roots are at the bottom and everything sits above what it builds on.
//! Each track is a branch, a column panel under its banner, with nested
//! tracks' banners stacked under their parent's. Every task is a card: a
//! task written under another is just one it needs, so it sits below it. A
//! card says who's on it, when it's expected, and links to its issues, pull
//! requests, and anything else written about it.
//!
//! Milestones are eras: each is a dashed line across the whole tree, with
//! its gate (label and progress) at the right end. Work sits in the band
//! under the line of the first milestone that needs it, and the band says
//! so without a line per task. A milestone gets a line from work only when
//! that work sits in an earlier band, or is a nice-to-have. Milestones go
//! in the order they're written, except that one whose work includes all of
//! another's goes above it. Nice-to-haves sit in the band of the milestone
//! that wants them. Work no milestone needs or wants, done or not, sits in
//! a layer of its own above the last line: nothing on a milestone waits on
//! it, so it never sits under what it builds on, and no era claims it.
//!
//! Lines are routed like circuit traces: up into the gap above a row,
//! across, and up into the next. A line that spans more than that climbs
//! the gutter between branches, so it never passes behind a card. Lines
//! leaving one card share one run, as do lines arriving at one from afar.
//!
//! The layout is in whole pixels, so drawing it needs no floating point.
//!
//! ```
//! use dagenda_core::tech_tree::TechTree;
//!
//! let plan = dagenda_core::parse(
//!     "roadmap
//!      track core [Core]
//!        [x] task parser [Parser]
//!        [ ] task resolver [Resolver]
//!          needs parser",
//! )?;
//! let tree = TechTree::new(&plan);
//! let card = |id: &str| tree.nodes.iter().find(|n| n.id.as_str() == id);
//! let (parser, resolver) = (card("parser").ok_or("no parser")?, card("resolver").ok_or("no resolver")?);
//! assert_eq!((parser.row, resolver.row), (0, 1));
//! assert!(resolver.y < parser.y, "the resolver builds on the parser, so it's higher up");
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```

mod route;
pub mod svg;

use crate::{
    colour::{Colour, Named},
    date::Date,
    handle::Handle,
    id::Id,
    plan::{Count, DocumentId, Plan, Progress, Readiness, Task, Track},
    reference::{Link, Ref},
    status::Status,
    text::{Note, Text},
};
use alloc::{
    collections::{BTreeMap, VecDeque},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

/// A plan laid out as a tech tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechTree {
    /// The first document's title.
    pub title: Option<Text>,

    /// The canvas's width, in pixels.
    pub width: u32,

    /// The canvas's height, in pixels.
    pub height: u32,

    /// The column panels, left to right: each track's own tasks, then tasks
    /// outside any track, then the column the milestones' gates sit in.
    pub branches: Vec<Branch>,

    /// The headings over the branches, one per track (spanning its nested
    /// tracks' branches too), and one for tasks outside any track.
    pub banners: Vec<Banner>,

    /// The eras, bottom to top: one under each milestone's line, then the
    /// layer of work on no milestone, if there is any. Empty when there are
    /// no milestones.
    pub bands: Vec<Band>,

    /// The top-level tasks, then the milestones.
    pub nodes: Vec<Node>,

    /// What waits on what, from prerequisite to dependent, leaving out any
    /// from work to the milestone whose band it sits in (the band says it),
    /// and any line a longer path of drawn lines already implies (if A needs
    /// B and B needs C, a line from C to A would only add clutter). A path
    /// through a band's milestone doesn't hide a must-have line, since its
    /// first step isn't drawn: if Y needs both X and X's milestone, X gets a
    /// line to Y. A nice-to-have line goes if the milestone needs the work
    /// anyway, by any path.
    pub edges: Vec<Edge>,

    /// The must-have lines from work to the milestone whose band it sits
    /// in: not drawn, since the band says it, and not routed, but there for
    /// whatever follows the graph (a longer path through one can leave a
    /// drawn line out).
    pub implied: Vec<Edge>,
}

impl TechTree {
    /// Lay out a plan.
    #[must_use]
    pub fn new(plan: &Plan) -> Self {
        let (mut nodes, index) = nodes(plan);
        let edges = edges(plan, &index);
        let tiers = tiers(&nodes, &edges);
        for (index, (node, &row)) in nodes.iter_mut().zip(&tiers.rows).enumerate() {
            node.row = row;
            if matches!(node.shape, Shape::Task { .. }) {
                node.era = tiers
                    .bands
                    .get(index)
                    .and_then(|&band| tiers.milestones.get(band))
                    .copied();
            }
        }
        // The band says what's in it, so those lines aren't drawn, and a
        // path through one doesn't stand in for a must-have line that is.
        // A nice-to-have line goes whenever its milestone needs the work
        // anyway, by any path.
        let every = successors(nodes.len(), &edges);
        let (members, drawn): (Vec<Edge>, Vec<Edge>) = edges
            .into_iter()
            .partition(|edge| tiers.in_own_band(&nodes, edge));
        let visible = successors(nodes.len(), &drawn);
        let implied = reduce(&every, members);
        let edges: Vec<Edge> = drawn
            .into_iter()
            .filter(|edge| !self::implied(if edge.wanted { &every } else { &visible }, edge))
            .collect();

        let (kinds, banners, branch_of) = branches(plan, &nodes);
        for (node, branch) in nodes.iter_mut().zip(branch_of) {
            node.branch = branch;
        }
        order(&mut nodes, &edges);

        let title = plan.title(DocumentId::new(0)).cloned();
        let top = if title.is_some() {
            MARGIN + TITLE
        } else {
            MARGIN
        };
        let courses = route::courses(&nodes, &edges, &tiers.gates);
        let room = route::Room::new(&edges, &courses, nodes.len(), kinds.len());
        let mut frame = place(&mut nodes, kinds, &banners, &tiers, top, &room);
        let edges = route::route(&mut nodes, edges, &courses, &mut frame, &tiers);

        let title_width = title.as_ref().map_or(0, |title| {
            MARGIN * 2 + text_width(title.as_str(), TITLE_FONT)
        });
        Self {
            title,
            width: frame.width.max(title_width),
            height: frame.height,
            branches: frame.branches,
            banners: frame.banners,
            bands: frame.bands,
            nodes,
            edges,
            implied,
        }
    }
}

/// A column panel holding cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Whose cards it holds.
    pub kind: BranchKind,

    /// Its track's colour: the one it sets, or the next free one in the
    /// palette, in the order tracks are written.
    pub colour: Option<Colour>,

    /// Its left edge.
    pub x: u32,

    /// Its top edge.
    pub y: u32,

    /// Its width.
    pub width: u32,

    /// Its height.
    pub height: u32,
}

/// Whose cards a branch holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchKind {
    /// A track's own tasks (its nested tracks have branches of their own).
    Track(Id),

    /// Tasks outside any track.
    Untracked,

    /// The milestones' gates, each on its line.
    Milestones,
}

/// A heading over one or more branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    /// What it heads.
    pub kind: BranchKind,

    /// Its text: the track's label, or its id.
    pub heading: String,

    /// The track's DRI.
    pub dri: Option<Handle>,

    /// Its track's colour: the one it sets, or the next free one in the
    /// palette, in the order tracks are written.
    pub colour: Option<Colour>,

    /// How deeply its track is nested: banners stack, outermost at the top.
    pub depth: usize,

    /// Its left edge.
    pub x: u32,

    /// Its top edge.
    pub y: u32,

    /// Its width: across its own branch and its nested tracks'.
    pub width: u32,

    /// Its height.
    pub height: u32,
}

/// An era: the band of the tree under a milestone's line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    /// The milestone whose line tops it, an index into [`TechTree::nodes`];
    /// `None` for the layer of work on no milestone.
    pub milestone: Option<usize>,

    /// Its top edge: its milestone's line, or the top of the tree.
    pub top: u32,

    /// Its bottom edge: the line below, or the bottom of the tree.
    pub bottom: u32,

    /// Whether its milestone is reached.
    pub reached: bool,
}

/// A card on the canvas: a top-level task, or a milestone's gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The task's or milestone's id.
    pub id: Id,

    /// What it is, and how far along.
    pub shape: Shape,

    /// Its label (or its id), wrapped to fit.
    pub lines: Vec<String>,

    /// A smaller line under the label: a milestone's progress (all the work
    /// behind it) and target, or how much of what a task waits on is done.
    pub caption: Option<String>,

    /// What it says under that, a row at a time: who's on it, when it's
    /// expected, and its links.
    pub facts: Vec<Vec<Fact>>,

    /// Its notes, for a tooltip.
    pub notes: Vec<Note>,

    /// Its branch, an index into [`TechTree::branches`].
    pub branch: usize,

    /// Its row, counted from the bottom: tiers within each band, and a row
    /// of its own for each milestone's gate.
    pub row: usize,

    /// For a task, the milestone whose band it sits in, an index into
    /// [`TechTree::nodes`]: `None` in the layer of work on no milestone, or
    /// with no milestones.
    pub era: Option<usize>,

    /// Its left edge.
    pub x: u32,

    /// Its top edge.
    pub y: u32,

    /// Its width.
    pub width: u32,

    /// Its height. A task's status badge hangs half below its bottom edge.
    pub height: u32,
}

impl Node {
    /// Its horizontal middle. Lines leave and arrive spread either side of
    /// it, a port each.
    #[must_use]
    pub const fn middle(&self) -> u32 {
        self.x + self.width / 2
    }

    /// Where a line into its middle arrives, from below: the bottom of a
    /// task's badge, or the bottom of a milestone's gate.
    #[must_use]
    pub const fn entry(&self) -> u32 {
        match self.shape {
            Shape::Task { .. } => self.y + self.height + BADGE_RADIUS,
            Shape::Milestone { .. } => self.y + self.height,
        }
    }
}

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A task.
    Task {
        /// Its status box.
        status: Status,

        /// Whether it waits on something unfinished: locked, in game terms.
        locked: bool,

        /// Everything it needs, directly or through other work, and how
        /// much of that is done. See [`Plan::work`].
        below: Count,
    },

    /// A milestone's gate.
    Milestone {
        /// How many of its members are finished.
        progress: Progress,

        /// When it should be reached.
        target: Option<Date>,
    },
}

/// Something a card says about its task, under the label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    /// Who's on it: the task's DRI.
    Dri(Handle),

    /// When it's expected to land.
    Est(Date),

    /// When it should land.
    Target(Date),

    /// An issue or pull request.
    Ref(Ref),

    /// Any other page about it.
    Link(Link),
}

impl Fact {
    /// What the card says: `@alice`, `est 2026-11-19`, `PR #7`, and so on.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Dri(dri) => dri.to_string(),
            Self::Est(date) => format!("est {date}"),
            Self::Target(date) => format!("target {date}"),
            Self::Ref(reference) => reference.short(),
            Self::Link(link) => link.name(),
        }
    }

    /// Where it goes when clicked, if anywhere.
    #[must_use]
    pub fn url(&self) -> Option<String> {
        match self {
            Self::Ref(reference) => Some(reference.url()),
            Self::Link(link) => Some(String::from(link.url.as_str())),
            Self::Dri(_) | Self::Est(_) | Self::Target(_) => None,
        }
    }
}

/// A line from a prerequisite up to what waits on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    /// The prerequisite, an index into [`TechTree::nodes`].
    pub from: usize,

    /// What waits on it.
    pub to: usize,

    /// A milestone's nice-to-have.
    pub wanted: bool,

    /// Whether the prerequisite is finished.
    pub met: bool,

    /// The corners of its path, from the top of the prerequisite to the
    /// bottom of what waits on it, each turn a right angle.
    pub route: Vec<Point>,
}

/// A point on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    /// Pixels from the left.
    pub x: u32,

    /// Pixels from the top.
    pub y: u32,
}

/// Text sizes, in pixels.
const FONT: u32 = 13;
const SMALL_FONT: u32 = 11;
const TITLE_FONT: u32 = 18;

/// Space around the canvas, and above it for a title.
const MARGIN: u32 = 16;
const TITLE: u32 = 34;

/// A card: its width, padding, line height, caption height, and the status
/// badge on a task's bottom edge.
const CARD: u32 = 150;
const PAD_X: u32 = 12;
const PAD_Y: u32 = 10;
const LINE: u32 = 17;
const CAPTION: u32 = 15;
const FACT_LINE: u32 = 14;
const FACT_GAP: u32 = 4;

/// Between facts on a row, and before a banner's DRI: non-breaking spaces,
/// so no renderer collapses them at the edge of a `tspan`.
const SEPARATOR: &str = "\u{a0}·\u{a0}";
const BADGE_RADIUS: u32 = 10;

/// How far a milestone's pointed ends stick out.
const NOTCH: u32 = 12;

/// How many cards share a row in one branch before the row wraps onto a
/// second line.
const PER_ROW: usize = 3;

/// Gaps: between cards in a row, between a wrapped row's lines, between
/// rows (where lines run), and between branches (where long lines climb).
/// Rows and branches move apart when more lines need to run between them.
const NODE_GAP: u32 = 14;
const WRAP_GAP: u32 = 28;
const ROW_GAP: u32 = 56;
const GUTTER: u32 = 30;

/// Lines: how far apart side by side (ports on a card, lanes in a gutter),
/// how far apart their runs across a gap, and how far a port keeps from the
/// end of a card's edge.
const LANE: u32 = 8;
const TRACK: u32 = 10;
const CLEAR: u32 = 12;

/// A branch's padding at its sides, above its top row, and below its bottom
/// row (room for the badges).
const PANEL_SIDE: u32 = 14;
const PANEL_TOP: u32 = 28;
const PANEL_BOTTOM: u32 = 28;

/// Each level of banner, the space between them, and its heading's size
/// (set in capitals, slightly spaced out) and padding.
const BANNER: u32 = 30;
const BANNER_GAP: u32 = 6;
const BANNER_FONT: u32 = 12;
const BANNER_PAD: u32 = 12;

/// The tasks and the milestones, and where each id is among them.
fn nodes(plan: &Plan) -> (Vec<Node>, BTreeMap<Id, usize>) {
    let mut nodes = Vec::new();
    let mut index = BTreeMap::new();

    for task in plan.tasks() {
        index.insert(task.id.clone(), nodes.len());
        let shape = Shape::Task {
            status: task.status,
            locked: matches!(plan.readiness(&task.id), Some(Readiness::Blocked(_))),
            below: plan.work(&task.id).unwrap_or(Count { done: 0, total: 0 }),
        };
        let mut card = node(&task.id, task.label.as_ref(), shape, facts(task));
        card.notes = task.notes.iter().map(|note| note.node.clone()).collect();
        nodes.push(card);
    }

    for milestone in plan.milestones() {
        index.insert(milestone.id.clone(), nodes.len());
        let progress = plan.progress(&milestone.id).unwrap_or(Progress {
            work: Count { done: 0, total: 0 },
            needs: Count { done: 0, total: 0 },
            wants: Count { done: 0, total: 0 },
        });
        let shape = Shape::Milestone {
            progress,
            target: milestone.target.as_ref().map(|target| target.node),
        };
        let mut gate = node(&milestone.id, milestone.label.as_ref(), shape, Vec::new());
        gate.notes = milestone
            .notes
            .iter()
            .map(|note| note.node.clone())
            .collect();
        nodes.push(gate);
    }

    (nodes, index)
}

/// A card, sized to its text, not yet placed.
fn node(id: &Id, label: Option<&Text>, shape: Shape, facts: Vec<Fact>) -> Node {
    let lines = wrap(label.map_or(id.as_str(), Text::as_str), CARD - PAD_X * 2);
    let caption = match &shape {
        // How close it is to unlocking, while there's something to wait on
        Shape::Task { status, below, .. }
            if below.total > 0
                && !below.is_complete()
                && !matches!(status, Status::Done | Status::Dropped) =>
        {
            Some(format!("{} of {} done below", below.done, below.total))
        }
        Shape::Task { .. } => None,
        Shape::Milestone { progress, target } => {
            let state = if progress.is_reached() {
                String::from("reached")
            } else {
                format!("{} of {}", progress.work.done, progress.work.total)
            };
            Some(match target {
                Some(target) => format!("{state} · {target}"),
                None => state,
            })
        }
    };

    let text = lines
        .iter()
        .map(|line| text_width(line, FONT))
        .max()
        .unwrap_or(0);
    let caption_width = caption
        .as_ref()
        .map_or(0, |caption| text_width(caption, SMALL_FONT));
    let widest_fact = facts
        .iter()
        .map(|fact| text_width(&fact.text(), SMALL_FONT))
        .max()
        .unwrap_or(0);
    let inner = (text.max(caption_width).max(widest_fact) + PAD_X * 2).max(CARD);
    let facts = fact_rows(facts, inner - PAD_X * 2);
    let rows = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    let caption_height = if caption.is_some() { CAPTION } else { 0 };
    let fact_height = if facts.is_empty() {
        0
    } else {
        FACT_GAP
            + u32::try_from(facts.len())
                .unwrap_or(u32::MAX)
                .saturating_mul(FACT_LINE)
    };
    let (width, bottom) = match shape {
        Shape::Task { .. } => (inner, BADGE_RADIUS + 6),
        Shape::Milestone { .. } => (inner + NOTCH * 2, PAD_Y),
    };

    Node {
        id: id.clone(),
        shape,
        lines,
        caption,
        facts,
        notes: Vec::new(),
        branch: 0,
        row: 0,
        era: None,
        x: 0,
        y: 0,
        width,
        height: PAD_Y + rows.saturating_mul(LINE) + caption_height + fact_height + bottom,
    }
}

/// What a task's card says about it: who's on it, when it's expected and
/// should land, and its links.
fn facts(task: &Task) -> Vec<Fact> {
    task.dri
        .iter()
        .map(|dri| Fact::Dri(dri.node.clone()))
        .chain(task.est.as_ref().map(|est| Fact::Est(est.node)))
        .chain(task.target.as_ref().map(|target| Fact::Target(target.node)))
        .chain(
            task.refs
                .iter()
                .map(|reference| Fact::Ref(reference.node.clone())),
        )
        .chain(task.links.iter().map(|link| Fact::Link(link.node.clone())))
        .collect()
}

/// Facts in rows no wider than `max` pixels at [`SMALL_FONT`], with
/// [`SEPARATOR`] between them. A fact wider than `max` gets a row of its own.
fn fact_rows(facts: Vec<Fact>, max: u32) -> Vec<Vec<Fact>> {
    let separator = text_width(SEPARATOR, SMALL_FONT);
    let mut rows: Vec<Vec<Fact>> = Vec::new();
    let mut width = 0;
    for fact in facts {
        let fact_width = text_width(&fact.text(), SMALL_FONT);
        match rows.last_mut() {
            Some(row) if width + separator + fact_width <= max => {
                width += separator + fact_width;
                row.push(fact);
            }
            Some(_) | None => {
                width = fact_width;
                rows.push(vec![fact]);
            }
        }
    }
    rows
}

/// Every prerequisite-to-dependent line between nodes, once each, not yet
/// routed.
fn edges(plan: &Plan, index: &BTreeMap<Id, usize>) -> Vec<Edge> {
    let mut edges: BTreeMap<(usize, usize), Edge> = BTreeMap::new();
    let mut add = |from: &Id, to: &Id, wanted: bool| {
        let (Some(&from_node), Some(&to_node)) = (index.get(from), index.get(to)) else {
            return;
        };
        if from_node == to_node {
            return;
        }
        let met = plan.is_finished(from);
        edges
            .entry((from_node, to_node))
            .and_modify(|edge| {
                edge.wanted &= wanted;
                edge.met &= met;
            })
            .or_insert(Edge {
                from: from_node,
                to: to_node,
                wanted,
                met,
                route: Vec::new(),
            });
    };

    for task in plan.tasks() {
        for need in &task.needs {
            add(&need.node, &task.id, false);
        }
    }
    for milestone in plan.milestones() {
        for member in &milestone.needs {
            add(&member.node, &milestone.id, false);
        }
        for member in &milestone.wants {
            add(&member.node, &milestone.id, true);
        }
    }
    edges.into_values().collect()
}

/// Leave out every line that a longer path of must-haves through `graph`
/// already implies, the transitive reduction (as Graphviz's `tred` does).
/// Rows don't change, since a longer path already pushes the dependent
/// further up.
fn reduce(graph: &[Vec<usize>], edges: Vec<Edge>) -> Vec<Edge> {
    edges
        .into_iter()
        .filter(|edge| !implied(graph, edge))
        .collect()
}

/// Whether a path of must-haves through `graph`, other than `edge` itself,
/// leads from its prerequisite to its dependent.
fn implied(graph: &[Vec<usize>], edge: &Edge) -> bool {
    let others = graph
        .get(edge.from)
        .into_iter()
        .flatten()
        .copied()
        .filter(|&next| next != edge.to);
    reaches(graph, others, edge.to)
}

/// Each node's must-have successors.
fn successors(count: usize, edges: &[Edge]) -> Vec<Vec<usize>> {
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); count];
    for edge in edges.iter().filter(|edge| !edge.wanted) {
        if let Some(next) = successors.get_mut(edge.from) {
            next.push(edge.to);
        }
    }
    successors
}

/// Whether following `successors` from any of `starts` arrives at `target`.
fn reaches<I: Iterator<Item = usize>>(successors: &[Vec<usize>], starts: I, target: usize) -> bool {
    let mut seen = vec![false; successors.len()];
    let mut stack: Vec<usize> = starts.collect();
    while let Some(node) = stack.pop() {
        if node == target {
            return true;
        }
        let Some(seen) = seen.get_mut(node) else {
            continue;
        };
        if *seen {
            continue;
        }
        *seen = true;
        stack.extend(successors.get(node).into_iter().flatten().copied());
    }
    false
}

/// Everything reachable from `start` along `next`, `start` included.
fn reachable(next: &[Vec<usize>], start: usize) -> Vec<bool> {
    let mut seen = vec![false; next.len()];
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        let Some(seen) = seen.get_mut(node) else {
            continue;
        };
        if *seen {
            continue;
        }
        *seen = true;
        stack.extend(next.get(node).into_iter().flatten().copied());
    }
    seen
}

/// Where everything sits vertically: each node's row, which rows are a
/// milestone's gate row, each node's band, and each band's milestone.
#[derive(Debug)]
struct Tiers {
    /// Each node's row, counted from the bottom.
    rows: Vec<usize>,

    /// Whether each row holds only a milestone's gate.
    gates: Vec<bool>,

    /// Each node's band, from [`bands_of`].
    bands: Vec<usize>,

    /// Each band's milestone, as a node, bottom to top.
    milestones: Vec<usize>,
}

impl Tiers {
    /// Whether a must-have line runs from work to the milestone whose band
    /// it's in, which the band already shows.
    fn in_own_band(&self, nodes: &[Node], edge: &Edge) -> bool {
        let into_gate = nodes
            .get(edge.to)
            .is_some_and(|node| matches!(node.shape, Shape::Milestone { .. }));
        let from_task = nodes
            .get(edge.from)
            .is_some_and(|node| matches!(node.shape, Shape::Task { .. }));
        !edge.wanted
            && into_gate
            && from_task
            && self.bands.get(edge.from) == self.bands.get(edge.to)
    }
}

/// Each node's row and band. Within a band, each task's tier is one more
/// than the deepest work it waits on in that band. A band's tiers come
/// first, then its milestone's gate row. Nice-to-haves don't count: a plan
/// that resolves has no loops among its must-haves, but may among its
/// nice-to-haves.
fn tiers(nodes: &[Node], edges: &[Edge]) -> Tiers {
    let forward = successors(nodes.len(), edges);
    let predecessors = predecessors(&forward);
    let order = topological(&forward);
    let milestones = milestone_order(nodes, &predecessors);
    let bands = bands_of(nodes, edges, &predecessors, &order, &milestones);
    let local = local_tiers(nodes, &forward, &bands, &order);

    let mut depth = vec![0_usize; milestones.len() + 1];
    for (node, &band) in bands.iter().enumerate() {
        let is_task = nodes
            .get(node)
            .is_some_and(|node| matches!(node.shape, Shape::Task { .. }));
        if is_task && let Some(depth) = depth.get_mut(band) {
            *depth = (*depth).max(local.get(node).copied().unwrap_or(0) + 1);
        }
    }

    let mut base = Vec::with_capacity(depth.len());
    let mut gates = Vec::new();
    for (band, &depth) in depth.iter().enumerate() {
        base.push(gates.len());
        gates.extend(core::iter::repeat_n(false, depth));
        if band < milestones.len() {
            gates.push(true);
        }
    }

    let rows = (0..nodes.len())
        .map(|node| {
            let band = bands.get(node).copied().unwrap_or(0);
            let start = base.get(band).copied().unwrap_or(0);
            match nodes.get(node).map(|node| &node.shape) {
                Some(Shape::Milestone { .. }) => start + depth.get(band).copied().unwrap_or(0),
                Some(Shape::Task { .. }) | None => start + local.get(node).copied().unwrap_or(0),
            }
        })
        .collect();

    Tiers {
        rows,
        gates,
        bands,
        milestones,
    }
}

/// Each node's must-have predecessors along `forward`.
fn predecessors(forward: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut predecessors: Vec<Vec<usize>> = vec![Vec::new(); forward.len()];
    for (from, next) in forward.iter().enumerate() {
        for &to in next {
            if let Some(list) = predecessors.get_mut(to) {
                list.push(from);
            }
        }
    }
    predecessors
}

/// Every node, each after everything it waits on along `forward` (which
/// has no cycles).
fn topological(forward: &[Vec<usize>]) -> Vec<usize> {
    let mut waiting = vec![0_usize; forward.len()];
    for &to in forward.iter().flatten() {
        if let Some(waiting) = waiting.get_mut(to) {
            *waiting += 1;
        }
    }
    let mut ready: VecDeque<usize> = (0..forward.len())
        .filter(|&node| waiting.get(node) == Some(&0))
        .collect();
    let mut order = Vec::with_capacity(forward.len());
    while let Some(node) = ready.pop_front() {
        order.push(node);
        for &next in forward.get(node).into_iter().flatten() {
            if let Some(count) = waiting.get_mut(next) {
                *count -= 1;
                if *count == 0 {
                    ready.push_back(next);
                }
            }
        }
    }
    order
}

/// The milestones, bottom band first: in the order they're written, except
/// that a milestone goes above any it needs, and above any other whose work
/// it needs all of, and more.
fn milestone_order(nodes: &[Node], predecessors: &[Vec<usize>]) -> Vec<usize> {
    let milestones: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| matches!(node.shape, Shape::Milestone { .. }))
        .map(|(index, _)| index)
        .collect();
    let needs: Vec<Vec<bool>> = milestones
        .iter()
        .map(|&milestone| {
            let mut needs = reachable(predecessors, milestone);
            if let Some(own) = needs.get_mut(milestone) {
                *own = false;
            }
            needs
        })
        .collect();

    // Either way, the upper one needs strictly more, so this can't loop
    let builds_on = |upper: usize, lower: usize| {
        let (Some(up), Some(low), Some(&low_node)) =
            (needs.get(upper), needs.get(lower), milestones.get(lower))
        else {
            return false;
        };
        let superset =
            up != low && low.contains(&true) && low.iter().zip(up).all(|(&low, &up)| up || !low);
        upper != lower && (up.get(low_node).copied().unwrap_or(false) || superset)
    };

    let mut placed = vec![false; milestones.len()];
    let mut order = Vec::with_capacity(milestones.len());
    for _ in 0..milestones.len() {
        let next = (0..milestones.len()).find(|&candidate| {
            placed.get(candidate) == Some(&false)
                && (0..milestones.len())
                    .all(|lower| placed.get(lower) == Some(&true) || !builds_on(candidate, lower))
        });
        let Some(next) = next else { break };
        if let Some(done) = placed.get_mut(next) {
            *done = true;
        }
        order.extend(milestones.get(next).copied());
    }
    order
}

/// Each node's band, an index into `milestones`, or one past the last for
/// the layer of work on no milestone. A milestone's band is its own. Work
/// goes in the band of the first milestone that needs it, directly or
/// through other work. Work no milestone needs, but one wants (directly, or
/// through work it wants), goes in that one's band, or later if what it
/// waits on is. Anything else goes in the layer on top, done or not: putting
/// finished work as early as it could go put it in the first era, which
/// claimed it for the first milestone.
fn bands_of(
    nodes: &[Node],
    edges: &[Edge],
    predecessors: &[Vec<usize>],
    order: &[usize],
    milestones: &[usize],
) -> Vec<usize> {
    let beyond = milestones.len();
    let needed_by: Vec<Vec<bool>> = milestones
        .iter()
        .map(|&milestone| reachable(predecessors, milestone))
        .collect();
    let wanted_by: Vec<Vec<bool>> = milestones
        .iter()
        .map(|&milestone| {
            let mut wanted = vec![false; nodes.len()];
            for edge in edges.iter().filter(|e| e.wanted && e.to == milestone) {
                for (wanted, reached) in wanted.iter_mut().zip(reachable(predecessors, edge.from)) {
                    *wanted |= reached;
                }
            }
            wanted
        })
        .collect();
    let first = |sets: &[Vec<bool>], node: usize| {
        sets.iter()
            .position(|set| set.get(node).copied().unwrap_or(false))
    };
    let is_gate = |node: usize| {
        nodes
            .get(node)
            .is_some_and(|node| matches!(node.shape, Shape::Milestone { .. }))
    };

    let mut bands = vec![beyond; nodes.len()];
    for &node in order {
        // Above every gate it waits on, and no lower than other work
        let floor = predecessors
            .get(node)
            .into_iter()
            .flatten()
            .map(|&before| bands.get(before).copied().unwrap_or(0) + usize::from(is_gate(before)))
            .max()
            .unwrap_or(0);
        let band = match nodes.get(node).map(|node| &node.shape) {
            Some(Shape::Milestone { .. }) => milestones.iter().position(|&m| m == node),
            Some(Shape::Task { .. }) => first(&needed_by, node)
                .or_else(|| first(&wanted_by, node).map(|band| band.max(floor))),
            None => None,
        };
        if let Some(slot) = bands.get_mut(node) {
            *slot = band.unwrap_or(beyond).min(beyond);
        }
    }
    bands
}

/// Each task's tier within its band: one more than the deepest work it waits
/// on in the same band.
fn local_tiers(
    nodes: &[Node],
    forward: &[Vec<usize>],
    bands: &[usize],
    order: &[usize],
) -> Vec<usize> {
    let is_task = |node: usize| {
        nodes
            .get(node)
            .is_some_and(|node| matches!(node.shape, Shape::Task { .. }))
    };
    let mut tiers = vec![0_usize; nodes.len()];
    for &node in order.iter().filter(|&&node| is_task(node)) {
        let tier = tiers.get(node).copied().unwrap_or(0);
        for &next in forward.get(node).into_iter().flatten() {
            if is_task(next)
                && bands.get(node) == bands.get(next)
                && let Some(next_tier) = tiers.get_mut(next)
            {
                *next_tier = (*next_tier).max(tier + 1);
            }
        }
    }
    tiers
}

/// A banner before placing: what it heads, its text and depth, the branches
/// it spans, and whether it reaches up to them (it does unless nested
/// banners sit between).
#[derive(Debug, Clone)]
struct BannerSpec {
    kind: BranchKind,
    heading: String,
    dri: Option<Handle>,
    colour: Option<Colour>,
    depth: usize,
    first: usize,
    last: usize,
    reaches: bool,
}

/// Each track's colour: the one it sets, or the next in the palette,
/// skipping any another track claims by name (all of them, if every one is
/// claimed).
struct Palette {
    free: Vec<Named>,
    given: usize,
}

impl Palette {
    fn new(plan: &Plan) -> Self {
        let claimed: Vec<Named> = plan
            .tracks()
            .iter()
            .filter_map(|track| match track.colour.as_ref()?.node {
                Colour::Named(named) => Some(named),
                Colour::Rgb(_) => None,
            })
            .collect();
        let free: Vec<Named> = Named::ALL
            .into_iter()
            .filter(|named| !claimed.contains(named))
            .collect();
        Self {
            free: if free.is_empty() {
                Named::ALL.to_vec()
            } else {
                free
            },
            given: 0,
        }
    }

    fn colour(&mut self, track: &Track) -> Colour {
        if let Some(colour) = &track.colour {
            return colour.node;
        }
        let next = self.free.get(self.given % self.free.len()).copied();
        self.given += 1;
        Colour::Named(next.unwrap_or(Named::Teal))
    }
}

/// The branches, left to right, the banners under them, and each node's
/// branch.
///
/// A track gets a branch if it has tasks of its own, or no nested tracks
/// (so an empty track still shows). Every track gets a banner across its
/// own branch and its nested tracks'. The milestones' gates get a column of
/// their own, with no banner: their lines say what they are.
fn branches(plan: &Plan, nodes: &[Node]) -> (Vec<BranchKind>, Vec<BannerSpec>, Vec<usize>) {
    let mut kinds = Vec::new();
    let mut banners: Vec<BannerSpec> = Vec::new();
    let mut track_branch = BTreeMap::new();

    let mut stack: Vec<_> = plan
        .tracks()
        .iter()
        .filter(|track| track.parent.is_none())
        .rev()
        .map(|track| (track, 0_usize))
        .collect();
    // Banners close once their nested tracks are done: (banner, depth)
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut steps = 0;
    let mut palette = Palette::new(plan);
    while let Some((track, depth)) = stack.pop() {
        steps += 1;
        if steps > plan.tracks().len() {
            break;
        }
        while open
            .last()
            .is_some_and(|&(_, open_depth)| open_depth >= depth)
        {
            open.pop();
        }

        let children: Vec<_> = plan
            .tracks()
            .iter()
            .filter(|child| child.parent.as_ref() == Some(&track.id))
            .collect();
        let has_tasks = nodes.iter().any(|node| {
            matches!(node.shape, Shape::Task { .. })
                && plan.task(&node.id).and_then(|task| task.track.as_ref()) == Some(&track.id)
        });

        let first = kinds.len();
        if has_tasks || children.is_empty() {
            track_branch.insert(track.id.clone(), kinds.len());
            kinds.push(BranchKind::Track(track.id.clone()));
        }
        for &(banner, _) in &open {
            if let Some(banner) = banners.get_mut(banner) {
                banner.last = kinds.len().saturating_sub(1).max(banner.first);
            }
        }
        open.push((banners.len(), depth));
        banners.push(BannerSpec {
            kind: BranchKind::Track(track.id.clone()),
            heading: String::from(track.label.as_ref().map_or(track.id.as_str(), Text::as_str)),
            dri: track.dri.as_ref().map(|dri| dri.node.clone()),
            colour: Some(palette.colour(track)),
            depth,
            first,
            last: kinds.len().saturating_sub(1).max(first),
            reaches: children.is_empty(),
        });
        stack.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
    }

    let untracked = nodes.iter().any(|node| {
        matches!(node.shape, Shape::Task { .. })
            && plan
                .task(&node.id)
                .and_then(|task| task.track.as_ref())
                .is_none_or(|track| !track_branch.contains_key(track))
    });
    let untracked_branch = kinds.len();
    if untracked {
        banners.push(BannerSpec {
            kind: BranchKind::Untracked,
            heading: String::from("Other tasks"),
            dri: None,
            colour: None,
            depth: 0,
            first: kinds.len(),
            last: kinds.len(),
            reaches: true,
        });
        kinds.push(BranchKind::Untracked);
    }

    let milestone_branch = kinds.len();
    if nodes
        .iter()
        .any(|node| matches!(node.shape, Shape::Milestone { .. }))
    {
        kinds.push(BranchKind::Milestones);
    }

    let branch_of = nodes
        .iter()
        .map(|node| match node.shape {
            Shape::Milestone { .. } => milestone_branch,
            Shape::Task { .. } => plan
                .task(&node.id)
                .and_then(|task| task.track.as_ref())
                .and_then(|track| track_branch.get(track).copied())
                .unwrap_or(untracked_branch),
        })
        .collect();
    (kinds, banners, branch_of)
}

/// Order the cards that share a branch and a row so lines cross less: each
/// goes by the average position of what it waits on. Leaves each card's
/// place in its cell in `x`, for [`place`].
fn order(nodes: &mut [Node], edges: &[Edge]) {
    let rows = nodes
        .iter()
        .map(|node| node.row)
        .max()
        .map_or(0, |max| max + 1);
    let mut slot: Vec<usize> = (0..nodes.len()).collect();

    for row in 0..rows {
        let mut cells: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate().filter(|(_, n)| n.row == row) {
            let (sum, count) = edges
                .iter()
                .filter(|edge| edge.to == index)
                .filter_map(|edge| {
                    let from = nodes.get(edge.from)?;
                    Some(from.branch * 10_000 + slot.get(edge.from).copied().unwrap_or(0))
                })
                .fold((0, 0), |(sum, count), key| (sum + key, count + 1));
            let key = if count == 0 {
                node.branch * 10_000 + index
            } else {
                sum / count
            };
            cells.entry(node.branch).or_default().push((key, index));
        }
        for cell in cells.values_mut() {
            cell.sort_unstable();
            for (position, &(_, index)) in cell.iter().enumerate() {
                if let Some(slot) = slot.get_mut(index) {
                    *slot = position;
                }
            }
        }
    }

    for (node, slot) in nodes.iter_mut().zip(slot) {
        node.x = u32::try_from(slot).unwrap_or(u32::MAX);
    }
}

/// Where everything ended up: the branches, each gutter's width (from the
/// one left of the first branch), the banners, and bands, each row's top
/// and height, the branches' top and bottom, and the canvas's size.
#[derive(Debug)]
struct Frame {
    branches: Vec<Branch>,
    gutters: Vec<u32>,
    banners: Vec<Banner>,
    bands: Vec<Band>,
    rows: Vec<(u32, u32)>,
    panel_top: u32,
    panel_bottom: u32,
    width: u32,
    height: u32,
}

impl Frame {
    /// The top and bottom of a gap between rows, leaving room for the
    /// badges hanging below the row above it.
    fn bounds(&self, gap: Gap) -> (u32, u32) {
        let below_row = |row: usize| {
            self.rows.get(row).map_or(self.panel_top, |&(top, height)| {
                top + height + BADGE_RADIUS + 2
            })
        };
        match gap {
            Gap::BeforeFirst => (below_row(0), self.panel_bottom),
            Gap::After(row) => {
                let top = if row + 1 < self.rows.len() {
                    below_row(row + 1)
                } else {
                    self.panel_top
                };
                let bottom = self
                    .rows
                    .get(row)
                    .map_or(self.panel_bottom, |&(top, _)| top);
                (top.min(bottom), bottom)
            }
        }
    }

    /// The middle of a gutter: the one left of branch `gutter`, or the one
    /// right of the last branch.
    fn gutter(&self, gutter: usize) -> u32 {
        let half = self.gutters.get(gutter).copied().unwrap_or(GUTTER) / 2;
        self.branches.get(gutter).map_or_else(
            || {
                self.branches
                    .last()
                    .map_or(MARGIN, |last| last.x + last.width + half)
            },
            |branch| branch.x.saturating_sub(half),
        )
    }

    /// A row's vertical middle, where a milestone's line runs.
    fn middle(&self, row: usize) -> u32 {
        self.rows
            .get(row)
            .map_or(self.panel_top, |&(top, height)| top + height / 2)
    }
}

/// The cards in each branch's row, in the order `order` left in `x`.
type Cells = BTreeMap<(usize, usize), Vec<usize>>;

/// Give every card, branch, banner, and band its place, starting `top`
/// pixels down, with the `room` the lines need. `order` left each card's
/// place in its cell in `x`.
fn place(
    nodes: &mut [Node],
    kinds: Vec<BranchKind>,
    banners: &[BannerSpec],
    tiers: &Tiers,
    top: u32,
    room: &route::Room,
) -> Frame {
    let mut cells: Cells = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        cells
            .entry((node.branch, node.row))
            .or_default()
            .push(index);
    }
    for cell in cells.values_mut() {
        cell.sort_by_key(|&index| nodes.get(index).map_or(0, |node| node.x));
    }
    let fans = &room.fans;
    let (mut inner, heights) = measure(nodes, &cells, fans, kinds.len(), tiers.gates.len());
    fit_banners(&mut inner, banners, &room.gutters);

    // Banners first, stacked outermost at the top; then the rows, the
    // highest at the top: time goes up
    let depth = banners
        .iter()
        .map(|banner| banner.depth)
        .max()
        .map_or(0, |max| max + 1);
    let banner_top = top;
    let panel_top = top + u32::try_from(depth).unwrap_or(0).saturating_mul(BANNER);
    let mut rows = vec![(0, 0); heights.len()];
    let mut y = panel_top + PANEL_TOP;
    for (row, height) in heights.iter().enumerate().rev() {
        if let Some(slot) = rows.get_mut(row) {
            *slot = (y, *height);
        }
        y += height + ROW_GAP;
    }
    let panel_bottom = if heights.is_empty() {
        panel_top + PANEL_TOP + PANEL_BOTTOM
    } else {
        y - ROW_GAP + PANEL_BOTTOM
    };

    let mut branches = Vec::with_capacity(kinds.len());
    let mut x = MARGIN;
    for (index, (kind, inner)) in kinds.into_iter().zip(&inner).enumerate() {
        let width = inner + PANEL_SIDE * 2;
        let colour = banners
            .iter()
            .find(|banner| banner.kind == kind)
            .and_then(|banner| banner.colour);
        branches.push(Branch {
            kind,
            colour,
            x,
            y: panel_top,
            width,
            height: panel_bottom - panel_top,
        });
        x += width + room.gutters.get(index + 1).copied().unwrap_or(GUTTER);
    }

    put(nodes, &cells, fans, &branches, &rows);
    let banners = place_banners(banners, &branches, banner_top, panel_top);
    let mut frame = Frame {
        // Room for the gutter beyond the last branch, where lines can climb
        width: x + MARGIN / 2,
        height: panel_bottom + MARGIN,
        branches,
        gutters: room.gutters.clone(),
        banners,
        bands: Vec::new(),
        rows,
        panel_top,
        panel_bottom,
    };
    frame.bands = bands(&frame, nodes, tiers);
    frame
}

/// The eras, bottom to top: each from the line below it (or the bottom of
/// the tree) up to its milestone's line (or the top of the tree).
fn bands(frame: &Frame, nodes: &[Node], tiers: &Tiers) -> Vec<Band> {
    if tiers.milestones.is_empty() {
        return Vec::new();
    }

    let mut bands = Vec::with_capacity(tiers.milestones.len() + 1);
    let mut bottom = frame.panel_bottom;
    for &milestone in &tiers.milestones {
        let row = tiers.rows.get(milestone).copied().unwrap_or(0);
        let top = frame.middle(row);
        let reached = nodes.get(milestone).is_some_and(
            |node| matches!(node.shape, Shape::Milestone { progress, .. } if progress.is_reached()),
        );
        bands.push(Band {
            milestone: Some(milestone),
            top,
            bottom,
            reached,
        });
        bottom = top;
    }
    let beyond = tiers.milestones.len();
    if tiers.bands.contains(&beyond) {
        bands.push(Band {
            milestone: None,
            top: frame.panel_top,
            bottom,
            reached: false,
        });
    }
    bands
}

/// How wide each branch's inside must be, and how tall each of `rows`
/// rows, with each cell wrapped onto at most two lines.
fn measure(
    nodes: &[Node],
    cells: &Cells,
    fans: &[usize],
    branches: usize,
    rows: usize,
) -> (Vec<u32>, Vec<u32>) {
    let mut inner = vec![CARD; branches];
    let mut heights = vec![0_u32; rows];
    for (&(branch, row), cell) in cells {
        let mut height = 0;
        for (line_index, line) in cell.chunks(per_line(cell.len())).enumerate() {
            let cards = line.iter().filter_map(|&index| nodes.get(index));
            height += cards.map(|node| node.height).max().unwrap_or(0);
            if line_index > 0 {
                height += WRAP_GAP;
            }
        }
        if let Some(inner) = inner.get_mut(branch) {
            *inner = (*inner).max(cell_width(nodes, cell, fans));
        }
        if let Some(tallest) = heights.get_mut(row) {
            *tallest = (*tallest).max(height);
        }
    }
    (inner, heights)
}

/// Widen branches so every banner's heading fits across the branches it
/// spans (and the `gutters` between), narrowest banners first, widening the
/// last branch it spans.
fn fit_banners(inner: &mut [u32], banners: &[BannerSpec], gutters: &[u32]) {
    let mut specs: Vec<&BannerSpec> = banners.iter().collect();
    specs.sort_by_key(|spec| spec.last.saturating_sub(spec.first));
    for spec in specs {
        let panels: u32 = inner
            .get(spec.first..=spec.last)
            .into_iter()
            .flatten()
            .map(|inner| inner + PANEL_SIDE * 2)
            .sum();
        let gutters: u32 = gutters
            .get(spec.first + 1..=spec.last)
            .into_iter()
            .flatten()
            .sum();
        let wanted = heading_width(&spec.heading, spec.dri.as_ref()) + BANNER_PAD * 2;
        if let Some(last) = inner.get_mut(spec.last) {
            *last += wanted.saturating_sub(panels + gutters);
        }
    }
}

/// Roughly how wide a banner's heading sets: in capitals, with 0.06em
/// between letters, then the DRI as written.
fn heading_width(heading: &str, dri: Option<&Handle>) -> u32 {
    let capitals = heading.to_uppercase();
    let letters = u32::try_from(capitals.chars().count()).unwrap_or(u32::MAX);
    let dri = dri.map_or(0, |dri| {
        text_width(&format!("{SEPARATOR}{dri}"), BANNER_FONT)
    });
    text_width(&capitals, BANNER_FONT) + letters.saturating_mul(BANNER_FONT * 6).div_ceil(100) + dri
}

/// How many cards share a line in a cell of `count`: [`PER_ROW`], or more,
/// so a cell never wraps onto more than two lines.
const fn per_line(count: usize) -> usize {
    let half = count.div_ceil(2);
    if half > PER_ROW { half } else { PER_ROW }
}

/// The distance between card slots in a cell: its widest card, and a gap.
/// Every card sits in the middle of a slot.
fn pitch(nodes: &[Node], cell: &[usize], fans: &[usize]) -> u32 {
    cell.iter()
        .filter_map(|&index| nodes.get(index))
        .map(|node| node.width)
        .max()
        .unwrap_or(CARD)
        + gap(cell, fans)
}

/// The gap between cards in a cell. A wrapped cell's lines climb between
/// the cards of its other line (see [`stagger`]), so its gaps are wide
/// enough for its widest fan of lines, a [`LANE`] apart.
fn gap(cell: &[usize], fans: &[usize]) -> u32 {
    if cell.len() <= per_line(cell.len()) {
        return NODE_GAP;
    }
    let fan = cell
        .iter()
        .filter_map(|&index| fans.get(index))
        .max()
        .copied()
        .unwrap_or(1);
    NODE_GAP
        + u32::try_from(fan.saturating_sub(1)).map_or(u32::MAX, |more| more.saturating_mul(LANE))
}

/// How far a wrapped line is pushed right: half a slot on the second line,
/// so a line climbing from a card below passes between the cards above
/// rather than behind one.
const fn stagger(line_index: usize, pitch: u32) -> u32 {
    if line_index % 2 == 1 { pitch / 2 } else { 0 }
}

/// How wide a cell is: its widest line, counting the second line's stagger.
fn cell_width(nodes: &[Node], cell: &[usize], fans: &[usize]) -> u32 {
    let (pitch, gap) = (pitch(nodes, cell, fans), gap(cell, fans));
    cell.chunks(per_line(cell.len()))
        .enumerate()
        .map(|(line_index, line)| {
            let slots = u32::try_from(line.len()).unwrap_or(u32::MAX);
            stagger(line_index, pitch) + slots.saturating_mul(pitch).saturating_sub(gap)
        })
        .max()
        .unwrap_or(0)
}

/// Put each cell's cards in their branch and row: the cell centred, each
/// card in the middle of its slot, the second line staggered.
fn put(
    nodes: &mut [Node],
    cells: &Cells,
    fans: &[usize],
    branches: &[Branch],
    rows: &[(u32, u32)],
) {
    for (&(branch, row), cell) in cells {
        let (Some(panel), Some(&(row_top, _))) = (branches.get(branch), rows.get(row)) else {
            continue;
        };
        let (pitch, gap) = (pitch(nodes, cell, fans), gap(cell, fans));
        let room = panel.width - PANEL_SIDE * 2;
        let base = panel.x + PANEL_SIDE + room.saturating_sub(cell_width(nodes, cell, fans)) / 2;
        let mut line_top = row_top;
        for (line_index, line) in cell.chunks(per_line(cell.len())).enumerate() {
            let mut slot = base + stagger(line_index, pitch);
            let mut tallest = 0;
            for &index in line {
                if let Some(node) = nodes.get_mut(index) {
                    node.x = slot + (pitch - gap).saturating_sub(node.width) / 2;
                    node.y = line_top;
                    tallest = tallest.max(node.height);
                }
                slot += pitch;
            }
            line_top += tallest + WRAP_GAP;
        }
    }
}

/// Each banner over its branches, stacked by depth with the outermost at
/// the top, from `banner_top`; a banner with no nested banners under it
/// reaches down to its branches at `panel_top`.
fn place_banners(
    specs: &[BannerSpec],
    branches: &[Branch],
    banner_top: u32,
    panel_top: u32,
) -> Vec<Banner> {
    specs
        .iter()
        .filter_map(|spec| {
            let first = branches.get(spec.first)?;
            let last = branches.get(spec.last)?;
            let level = u32::try_from(spec.depth).unwrap_or(0);
            let y = banner_top + level.saturating_mul(BANNER);
            let bottom = if spec.reaches { panel_top } else { y + BANNER };
            Some(Banner {
                kind: spec.kind.clone(),
                heading: spec.heading.clone(),
                dri: spec.dri.clone(),
                colour: spec.colour,
                depth: spec.depth,
                x: first.x,
                y,
                width: last.x + last.width - first.x,
                height: bottom.saturating_sub(y + BANNER_GAP),
            })
        })
        .collect()
}

/// A gap between rows, where lines run across: the one above a row
/// (between it and the next), or the one under the bottom row. Higher gaps
/// sort later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Gap {
    BeforeFirst,
    After(usize),
}

/// Split `text` into lines no wider than `max` pixels at [`FONT`], at
/// spaces. A word wider than `max` gets a line of its own.
fn wrap(text: &str, max: u32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        if !line.is_empty() && text_width(&format!("{line} {word}"), FONT) > max {
            lines.push(core::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// Roughly how wide `text` sets at `size` pixels, for sizing boxes. There
/// are no font metrics here (the viewer's font draws it), so characters
/// fall into width classes that hold across common sans-serifs,
/// erring wide so text stays inside its box.
fn text_width(text: &str, size: u32) -> u32 {
    let hundredths: u32 = text.chars().map(advance).sum();
    hundredths.saturating_mul(size).div_ceil(100)
}

/// One character's advance, in hundredths of an em.
const fn advance(c: char) -> u32 {
    match c {
        // Combining marks, variation selectors, and joiners
        '\u{0300}'..='\u{036f}' | '\u{200b}'..='\u{200f}' | '\u{fe00}'..='\u{fe0f}' => 0,
        ' ' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | 'i' | 'j' | 'l' | 'I' => 30,
        '(' | ')' | '[' | ']' | 'f' | 't' | 'r' | '-' => 38,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 85,
        'A'..='Z' => 66,
        c if c.is_ascii() => 56,
        c if wide(c) => 105,
        // Other scripts: accented Latin, Greek, Cyrillic, and so on
        _ => 62,
    }
}

/// East Asian wide and fullwidth characters, and emoji: about an em each.
const fn wide(c: char) -> bool {
    matches!(
        c as u32,
        0x1100..=0x115f
            | 0x2600..=0x27bf
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe30..=0xfe4f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f000..=0x1faff
            | 0x20000..=0x3fffd
    )
}
