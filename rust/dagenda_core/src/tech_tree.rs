//! The tech tree: tasks and milestones laid out by what they need, in lanes
//! by track.
//!
//! Work flows left to right: everything a node needs sits in a column to
//! its left. Milestones take the top lane, so the gates read across the top.
//! Each track takes a lane below, nested tracks under their parent's
//! heading, and tasks outside any track take the last lane. Subtasks fold
//! into their top-level task, which shows how many of them are done.
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
//! let column = |id: &str| tree.nodes.iter().find(|n| n.id.as_str() == id).map(|n| n.column);
//! assert_eq!(column("parser"), Some(0));
//! assert_eq!(column("resolver"), Some(1));
//! # Ok::<(), dagenda_core::Error>(())
//! ```

pub mod svg;

use crate::{
    id::Id,
    plan::{Count, DocumentId, Plan, Progress},
    status::Status,
    text::Text,
};
use alloc::{
    collections::{BTreeMap, VecDeque},
    format,
    string::String,
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

    /// The lanes, top to bottom.
    pub lanes: Vec<Lane>,

    /// The top-level tasks, then the milestones.
    pub nodes: Vec<Node>,

    /// What waits on what, from prerequisite to dependent, leaving out any
    /// line a longer path already implies: if A needs B and B needs C, a
    /// line from C to A would only add clutter.
    pub edges: Vec<Edge>,
}

impl TechTree {
    /// Lay out a plan.
    #[must_use]
    pub fn new(plan: &Plan) -> Self {
        let (mut nodes, index) = nodes(plan);
        let edges = edges(plan, &index);
        let columns = columns(nodes.len(), &edges);
        for (node, column) in nodes.iter_mut().zip(columns) {
            node.column = column;
        }

        let edges = reduce(nodes.len(), edges);

        let (mut lanes, lane_of) = lanes(plan, &nodes);
        for (node, lane) in nodes.iter_mut().zip(lane_of) {
            node.lane = lane;
        }

        let title = plan.title(DocumentId::new(0)).cloned();
        let top = if title.is_some() {
            MARGIN + TITLE
        } else {
            MARGIN
        };
        order(&mut nodes, &edges);
        let (width, height) = place(&mut nodes, &mut lanes, top);
        let title_width = title.as_ref().map_or(0, |title| {
            MARGIN * 2 + text_width(title.as_str(), TITLE_FONT)
        });

        Self {
            title,
            width: width.max(title_width),
            height,
            lanes,
            nodes,
            edges,
        }
    }
}

/// A horizontal band: the milestones, a track, or tasks outside any track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    /// What it holds.
    pub kind: LaneKind,

    /// Its heading: the track's label, or its id.
    pub heading: String,

    /// How deeply its track is nested.
    pub depth: usize,

    /// Its top edge.
    pub y: u32,

    /// Its height.
    pub height: u32,
}

/// What a lane holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaneKind {
    /// Every milestone.
    Milestones,

    /// A track's own tasks (its nested tracks have lanes of their own).
    Track(Id),

    /// Tasks outside any track.
    Untracked,
}

/// A box on the canvas: a top-level task or a milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The task's or milestone's id.
    pub id: Id,

    /// What it is, and how far along.
    pub shape: Shape,

    /// Its label (or its id), wrapped to fit.
    pub lines: Vec<String>,

    /// A smaller line under the label: subtasks done, or a milestone's
    /// progress and target.
    pub caption: Option<String>,

    /// Its lane, an index into [`TechTree::lanes`].
    pub lane: usize,

    /// Its column, counted from the left: one more than the furthest-right
    /// thing it waits on.
    pub column: usize,

    /// Its left edge.
    pub x: u32,

    /// Its top edge.
    pub y: u32,

    /// Its width.
    pub width: u32,

    /// Its height.
    pub height: u32,
}

/// What a node is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// A top-level task.
    Task {
        /// Its status box.
        status: Status,

        /// How many of its subtasks, at any depth, are done.
        subtasks: Count,
    },

    /// A milestone.
    Milestone {
        /// How many of its members are finished.
        progress: Progress,

        /// When it should be reached, as written.
        target: Option<Text>,
    },
}

/// A line from a prerequisite to what waits on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    /// The prerequisite, an index into [`TechTree::nodes`].
    pub from: usize,

    /// What waits on it.
    pub to: usize,

    /// A milestone's nice-to-have, from below its cut line.
    pub wanted: bool,

    /// Whether the prerequisite is finished.
    pub met: bool,
}

/// Text sizes, in pixels.
const FONT: u32 = 13;
const SMALL_FONT: u32 = 11;
const TITLE_FONT: u32 = 18;

/// Space around the canvas, and above it for a title.
const MARGIN: u32 = 16;
const TITLE: u32 = 34;

/// A node's padding, line height, status box, and widest line of text.
const PAD_X: u32 = 10;
const PAD_Y: u32 = 8;
const LINE: u32 = 17;
const CAPTION: u32 = 15;
const BOX: u32 = 12;
const BOX_GAP: u32 = 7;
const MAX_TEXT: u32 = 190;

/// How far a milestone's pointed ends stick out.
const NOTCH: u32 = 10;

/// Gaps between columns, between nodes in one lane and column, and between
/// lanes; and the room a lane's heading takes.
const COLUMN_GAP: u32 = 56;
const CELL_GAP: u32 = 10;
const LANE_GAP: u32 = 6;
const HEADING: u32 = 26;
const LANE_PAD: u32 = 12;

/// The top-level tasks and the milestones, and where every task and
/// milestone id lands among them (a subtask lands on its top-level task).
fn nodes(plan: &Plan) -> (Vec<Node>, BTreeMap<Id, usize>) {
    let mut nodes = Vec::new();
    let mut index = BTreeMap::new();

    for task in plan.tasks().iter().filter(|task| task.parent.is_none()) {
        index.insert(task.id.clone(), nodes.len());
        let shape = Shape::Task {
            status: task.status,
            subtasks: subtasks(plan, &task.id),
        };
        nodes.push(node(&task.id, task.label.as_ref(), shape));
    }

    for milestone in plan.milestones() {
        index.insert(milestone.id.clone(), nodes.len());
        let progress = plan.progress(&milestone.id).unwrap_or(Progress {
            needs: Count { done: 0, total: 0 },
            wants: Count { done: 0, total: 0 },
        });
        let shape = Shape::Milestone {
            progress,
            target: milestone.target.as_ref().map(|target| target.node.clone()),
        };
        nodes.push(node(&milestone.id, milestone.label.as_ref(), shape));
    }

    for task in plan.tasks().iter().filter(|task| task.parent.is_some()) {
        if let Some(&node) = top(plan, &task.id).and_then(|top| index.get(top)) {
            index.insert(task.id.clone(), node);
        }
    }

    (nodes, index)
}

/// A node, sized to its text, not yet placed.
fn node(id: &Id, label: Option<&Text>, shape: Shape) -> Node {
    let lines = wrap(label.map_or(id.as_str(), Text::as_str), MAX_TEXT);
    let caption = match &shape {
        Shape::Task { subtasks, .. } if subtasks.total > 0 => {
            Some(format!("{} of {} subtasks", subtasks.done, subtasks.total))
        }
        Shape::Task { .. } => None,
        Shape::Milestone { progress, target } => {
            let state = if progress.is_reached() {
                String::from("reached")
            } else {
                format!("{} of {}", progress.needs.done, progress.needs.total)
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
    let lead = match shape {
        Shape::Task { .. } => BOX + BOX_GAP,
        Shape::Milestone { .. } => NOTCH,
    };
    let trail = match shape {
        Shape::Task { .. } => 0,
        Shape::Milestone { .. } => NOTCH,
    };
    let rows = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    let caption_height = if caption.is_some() { CAPTION } else { 0 };

    Node {
        id: id.clone(),
        shape,
        lines,
        caption,
        lane: 0,
        column: 0,
        x: 0,
        y: 0,
        width: PAD_X * 2 + lead + text.max(caption_width) + trail,
        height: PAD_Y * 2 + rows.saturating_mul(LINE) + caption_height,
    }
}

/// How many of a task's subtasks, at any depth, are done.
fn subtasks(plan: &Plan, id: &Id) -> Count {
    let mut count = Count { done: 0, total: 0 };
    let mut stack: Vec<&Id> = plan
        .task(id)
        .map(|task| task.subtasks.iter().collect())
        .unwrap_or_default();
    while let Some(subtask) = stack.pop() {
        if count.total > plan.tasks().len() {
            break;
        }
        if let Some(subtask) = plan.task(subtask) {
            count.total += 1;
            if subtask.status.is_finished() {
                count.done += 1;
            }
            stack.extend(&subtask.subtasks);
        }
    }
    count
}

/// The top-level task a subtask belongs to.
fn top<'a>(plan: &'a Plan, id: &'a Id) -> Option<&'a Id> {
    let mut current = id;
    for _ in 0..=plan.tasks().len() {
        match plan.task(current)?.parent.as_ref() {
            Some(parent) => current = parent,
            None => return Some(current),
        }
    }
    None
}

/// Every prerequisite-to-dependent line between nodes, once each.
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

/// Leave out every line that a longer path of must-haves already implies,
/// the transitive reduction (as Graphviz's `tred` does). Columns don't
/// change, since a longer path already pushes the dependent further right.
fn reduce(count: usize, edges: Vec<Edge>) -> Vec<Edge> {
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); count];
    for edge in edges.iter().filter(|edge| !edge.wanted) {
        if let Some(next) = successors.get_mut(edge.from) {
            next.push(edge.to);
        }
    }

    edges
        .into_iter()
        .filter(|edge| {
            let others = successors
                .get(edge.from)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&next| next != edge.to);
            !reaches(&successors, others, edge.to)
        })
        .collect()
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

/// Each node's column: one more than the furthest column it waits on.
///
/// Nice-to-haves don't count, so a milestone sits right after its
/// must-haves. Folding subtasks into their parents can make a loop (a
/// subtask that needs something that needs its parent), so lines that would
/// close one are left out of the count.
fn columns(count: usize, edges: &[Edge]) -> Vec<usize> {
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); count];
    for edge in edges.iter().filter(|edge| !edge.wanted) {
        if let Some(next) = successors.get_mut(edge.from) {
            next.push(edge.to);
        }
    }
    let forward = without_back_edges(&successors);

    let mut waiting = vec![0_usize; count];
    for &to in forward.iter().flatten() {
        if let Some(waiting) = waiting.get_mut(to) {
            *waiting += 1;
        }
    }

    let mut columns = vec![0_usize; count];
    let mut ready: VecDeque<usize> = (0..count)
        .filter(|&node| waiting.get(node) == Some(&0))
        .collect();
    while let Some(node) = ready.pop_front() {
        let column = columns.get(node).copied().unwrap_or(0);
        for &next in forward.get(node).into_iter().flatten() {
            if let Some(next_column) = columns.get_mut(next) {
                *next_column = (*next_column).max(column + 1);
            }
            if let Some(count) = waiting.get_mut(next) {
                *count -= 1;
                if *count == 0 {
                    ready.push_back(next);
                }
            }
        }
    }
    columns
}

/// `successors` without the edges a depth-first search finds pointing back
/// up its own path, which leaves no cycles.
fn without_back_edges(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Visit {
        New,
        Open,
        Done,
    }

    let mut visit = vec![Visit::New; successors.len()];
    let mut forward: Vec<Vec<usize>> = vec![Vec::new(); successors.len()];
    for root in 0..successors.len() {
        if visit.get(root) != Some(&Visit::New) {
            continue;
        }
        let mut stack = vec![(root, 0_usize)];
        if let Some(state) = visit.get_mut(root) {
            *state = Visit::Open;
        }
        while let Some((node, next)) = stack.last_mut() {
            let node = *node;
            let Some(&to) = successors.get(node).and_then(|edges| edges.get(*next)) else {
                if let Some(state) = visit.get_mut(node) {
                    *state = Visit::Done;
                }
                stack.pop();
                continue;
            };
            *next += 1;
            match visit.get(to).copied() {
                Some(Visit::Open) | None => {}
                Some(Visit::Done) => {
                    if let Some(edges) = forward.get_mut(node) {
                        edges.push(to);
                    }
                }
                Some(Visit::New) => {
                    if let Some(edges) = forward.get_mut(node) {
                        edges.push(to);
                    }
                    if let Some(state) = visit.get_mut(to) {
                        *state = Visit::Open;
                    }
                    stack.push((to, 0));
                }
            }
        }
    }
    forward
}

/// The lanes, and each node's lane.
fn lanes(plan: &Plan, nodes: &[Node]) -> (Vec<Lane>, Vec<usize>) {
    let mut lanes = Vec::new();
    let has_milestones = nodes
        .iter()
        .any(|node| matches!(node.shape, Shape::Milestone { .. }));
    if has_milestones {
        lanes.push(Lane {
            kind: LaneKind::Milestones,
            heading: String::from("Milestones"),
            depth: 0,
            y: 0,
            height: 0,
        });
    }

    let mut track_lane = BTreeMap::new();
    let roots: Vec<_> = plan
        .tracks()
        .iter()
        .filter(|track| track.parent.is_none())
        .collect();
    let mut stack: Vec<_> = roots.into_iter().rev().map(|track| (track, 0)).collect();
    while let Some((track, depth)) = stack.pop() {
        if lanes.len() > plan.tracks().len() + 2 {
            break;
        }
        track_lane.insert(track.id.clone(), lanes.len());
        lanes.push(Lane {
            kind: LaneKind::Track(track.id.clone()),
            heading: String::from(track.label.as_ref().map_or(track.id.as_str(), Text::as_str)),
            depth,
            y: 0,
            height: 0,
        });
        let children = plan
            .tracks()
            .iter()
            .filter(|child| child.parent.as_ref() == Some(&track.id));
        let children: Vec<_> = children.collect();
        stack.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
    }

    let untracked = lanes.len();
    let mut any_untracked = false;
    let lane_of = nodes
        .iter()
        .map(|node| match node.shape {
            Shape::Milestone { .. } => 0,
            Shape::Task { .. } => {
                let track = plan.task(&node.id).and_then(|task| task.track.as_ref());
                track
                    .and_then(|track| track_lane.get(track).copied())
                    .unwrap_or_else(|| {
                        any_untracked = true;
                        untracked
                    })
            }
        })
        .collect();

    if any_untracked {
        lanes.push(Lane {
            kind: LaneKind::Untracked,
            heading: String::from("Other tasks"),
            depth: 0,
            y: 0,
            height: 0,
        });
    }
    (lanes, lane_of)
}

/// Order the nodes that share a lane and a column so lines cross less: each
/// goes by the average position of what it waits on.
fn order(nodes: &mut [Node], edges: &[Edge]) {
    let columns = nodes
        .iter()
        .map(|node| node.column)
        .max()
        .map_or(0, |max| max + 1);
    let mut slot: Vec<usize> = (0..nodes.len()).collect();

    for column in 0..columns {
        let mut cells: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate().filter(|(_, n)| n.column == column) {
            let (sum, count) = edges
                .iter()
                .filter(|edge| edge.to == index)
                .filter_map(|edge| {
                    let from = nodes.get(edge.from)?;
                    Some(from.lane * 10_000 + slot.get(edge.from).copied().unwrap_or(0))
                })
                .fold((0, 0), |(sum, count), key| (sum + key, count + 1));
            let key = if count == 0 {
                node.lane * 10_000 + index
            } else {
                sum / count
            };
            cells.entry(node.lane).or_default().push((key, index));
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
        node.y = u32::try_from(slot).unwrap_or(u32::MAX);
    }
}

/// Give every node and lane its place, starting `top` pixels down, and
/// return the canvas's size. `order` left each node's position in its cell
/// in `y`.
fn place(nodes: &mut [Node], lanes: &mut [Lane], top: u32) -> (u32, u32) {
    let columns = nodes
        .iter()
        .map(|node| node.column)
        .max()
        .map_or(0, |max| max + 1);
    let mut widths = vec![0_u32; columns];
    for node in nodes.iter() {
        if let Some(width) = widths.get_mut(node.column) {
            *width = (*width).max(node.width);
        }
    }

    let mut lefts = Vec::with_capacity(columns);
    let mut x = MARGIN;
    for width in &widths {
        lefts.push(x);
        x += width + COLUMN_GAP;
    }
    let right = x.saturating_sub(COLUMN_GAP) + MARGIN;

    let mut y = top;
    let mut widest_heading = 0;
    for (lane_index, lane) in lanes.iter_mut().enumerate() {
        lane.y = y;
        let content = y + HEADING;
        let mut tallest = 0;
        for column in 0..columns {
            let mut cell: Vec<&mut Node> = nodes
                .iter_mut()
                .filter(|node| node.lane == lane_index && node.column == column)
                .collect();
            cell.sort_by_key(|node| node.y);
            let mut cursor = content;
            for node in cell {
                node.x = lefts.get(column).copied().unwrap_or(MARGIN);
                node.y = cursor;
                cursor += node.height + CELL_GAP;
            }
            tallest = tallest.max(cursor.saturating_sub(content + CELL_GAP));
        }

        lane.height = HEADING + if tallest > 0 { tallest + LANE_PAD } else { 0 };
        let indent = u32::try_from(lane.depth).unwrap_or(0).saturating_mul(12);
        widest_heading = widest_heading.max(MARGIN * 2 + indent + text_width(&lane.heading, FONT));
        y += lane.height + LANE_GAP;
    }

    (
        right.max(widest_heading),
        y.saturating_sub(LANE_GAP) + MARGIN,
    )
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
/// are no font metrics here (the viewer's font draws it), so, as in rain,
/// characters fall into width classes that hold across common sans-serifs,
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
