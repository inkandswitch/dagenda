//! Routing the lines between cards, circuit style.
//!
//! A line leaves the top of its prerequisite, runs across the gap above
//! that row, and climbs into the bottom of what waits on it. If anything but
//! open ground lies between (a row of gates is open to work, and a row of
//! work to a gate), it runs across to the gutter beside its target's branch
//! instead, along it, and across the gap under its target.
//!
//! No two lines share any stretch, so every line can be followed from end
//! to end:
//!
//! - Each line has its own port on each card it joins, spread along the
//!   card's edge in the order the lines head off, so they fan out without
//!   crossing.
//! - Each gap between rows is a channel. Each line's run across it gets a
//!   track of its own (runs that don't overlap can share one), ordered so
//!   that a run passes above a line dropping out of the gap, and below one
//!   climbing out of it, wherever it can.
//! - Each line along a gutter gets a lane of its own.
//! - Gaps and gutters grow to fit.
//!
//! ```text
//!         ┌─────────┐
//!         │    T    │     T needs A and B: a port each on T,
//!         └──┬───┬──┘     and a track each across the gap,
//!   ┌────────┘   │        A's above B's, so neither crosses
//!   │      ┌─────┘
//!   │      │
//! ┌─┴─┐  ┌─┴─┐
//! │ A │  │ B │
//! └───┘  └───┘
//! ```

use super::{
    BADGE_RADIUS, CLEAR, Edge, Frame, GUTTER, Gap, LANE, NOTCH, Node, Point, Shape, TRACK, Tiers,
    bands,
};
use alloc::{collections::BTreeMap, vec, vec::Vec};

/// How a line gets from its prerequisite up to what waits on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Course {
    /// Across the gap above the prerequisite's row, and straight up over
    /// open ground.
    Direct,

    /// Across to a gutter, along it, and across the gap under what waits.
    Gutter(usize),
}

/// Each line's course, or none for a line to a missing node.
pub(super) fn courses(nodes: &[Node], edges: &[Edge], gates: &[bool]) -> Vec<Option<Course>> {
    edges
        .iter()
        .map(|edge| {
            let (from, to) = (nodes.get(edge.from)?, nodes.get(edge.to)?);
            // Only gates sit in their column, so a row of gates is open to a
            // line into a task, and a row of work to a line into a gate
            let into_task = matches!(to.shape, Shape::Task { .. });
            let open = to.row > from.row
                && (from.row + 1..to.row)
                    .all(|row| gates.get(row).copied().unwrap_or(false) == into_task);
            Some(if open {
                Course::Direct
            } else if from.branch < to.branch {
                Course::Gutter(to.branch)
            } else {
                Course::Gutter(to.branch + 1)
            })
        })
        .collect()
}

/// What the lines need from the layout, known before anything is placed.
#[derive(Debug)]
pub(super) struct Room {
    /// The most lines leaving or arriving at each card: a wrapped cell's
    /// gaps must let that many through.
    pub(super) fans: Vec<usize>,

    /// Each gutter's width, from the one left of the first branch to the one
    /// right of the last: a lane for each line along it.
    pub(super) gutters: Vec<u32>,
}

impl Room {
    pub(super) fn new(
        edges: &[Edge],
        courses: &[Option<Course>],
        nodes: usize,
        branches: usize,
    ) -> Self {
        let (mut leaving, mut arriving) = (vec![0_usize; nodes], vec![0_usize; nodes]);
        let mut lanes = vec![0_u32; branches + 1];
        for (edge, course) in edges.iter().zip(courses) {
            let Some(course) = course else { continue };
            if let Some(count) = leaving.get_mut(edge.from) {
                *count += 1;
            }
            if let Some(count) = arriving.get_mut(edge.to) {
                *count += 1;
            }
            if let Course::Gutter(gutter) = course
                && let Some(count) = lanes.get_mut(*gutter)
            {
                *count += 1;
            }
        }
        Self {
            fans: leaving
                .into_iter()
                .zip(arriving)
                .map(|(leaving, arriving)| leaving.max(arriving))
                .collect(),
            gutters: lanes
                .into_iter()
                .map(|lanes| GUTTER.max(lanes.saturating_sub(1) * LANE + CLEAR * 2))
                .collect(),
        }
    }
}

/// Route every line: a port at each end, a track across each gap it uses,
/// and a lane along its gutter, if any. Rows move apart where a gap needs
/// more tracks than it has room for.
pub(super) fn route(
    nodes: &mut [Node],
    mut edges: Vec<Edge>,
    courses: &[Option<Course>],
    frame: &mut Frame,
    tiers: &Tiers,
) -> Vec<Edge> {
    let mut ports = Ports::new(nodes, &edges, courses, frame);
    ports.nudge(nodes, &edges, courses);
    let lanes = lanes(courses, &ports, frame);
    let tracks: BTreeMap<Gap, Tracks> = channels(nodes, &edges, courses, &ports, &lanes)
        .into_iter()
        .map(|(gap, runs)| (gap, Tracks::assign(&runs)))
        .collect();
    grow(frame, nodes, &tracks, tiers);

    for (line, (edge, course)) in edges.iter_mut().zip(courses).enumerate() {
        let (Some(course), Some(from), Some(to), Some(start), Some(end)) = (
            course,
            nodes.get(edge.from),
            nodes.get(edge.to),
            ports.start(line),
            ports.end(line),
        ) else {
            continue;
        };
        let height = |gap: Gap| {
            tracks
                .get(&gap)
                .and_then(|tracks| tracks.y(line, frame.bounds(gap)))
        };
        let first = Gap::After(from.row);
        let mut route = vec![Point {
            x: start,
            y: from.y,
        }];
        match course {
            Course::Direct => {
                if let Some(y) = height(first) {
                    route.extend([Point { x: start, y }, Point { x: end, y }]);
                }
            }
            Course::Gutter(_) => {
                if let (Some(lane), Some(across), Some(under)) = (
                    lanes.get(line).copied().flatten(),
                    height(first),
                    height(under(to.row)),
                ) {
                    route.extend([
                        Point {
                            x: start,
                            y: across,
                        },
                        Point { x: lane, y: across },
                        Point { x: lane, y: under },
                        Point { x: end, y: under },
                    ]);
                }
            }
        }
        route.push(Point {
            x: end,
            y: arrival(to, end),
        });
        route.dedup();
        edge.route = route;
    }
    edges
}

/// Where each line leaves its prerequisite and arrives at what waits on
/// it, as x positions, by line.
#[derive(Debug)]
struct Ports {
    starts: Vec<Option<u32>>,
    ends: Vec<Option<u32>>,
}

impl Ports {
    /// Spread each card's lines along its edge: those leaving in the order
    /// they head off across the gap above (toward their target, or their
    /// gutter), and those arriving in the order they come in.
    fn new(nodes: &[Node], edges: &[Edge], courses: &[Option<Course>], frame: &Frame) -> Self {
        let middle = |node: usize| nodes.get(node).map_or(0, Node::middle);
        let mut leaving: BTreeMap<usize, Vec<(u32, u32, usize)>> = BTreeMap::new();
        let mut arriving: BTreeMap<usize, Vec<(u32, u32, usize)>> = BTreeMap::new();
        for (line, (edge, course)) in edges.iter().zip(courses).enumerate() {
            let Some(course) = course else { continue };
            let via = match course {
                Course::Direct => None,
                Course::Gutter(gutter) => Some(frame.gutter(*gutter)),
            };
            let (from, to) = (middle(edge.from), middle(edge.to));
            leaving
                .entry(edge.from)
                .or_default()
                .push((via.unwrap_or(to), to, line));
            arriving
                .entry(edge.to)
                .or_default()
                .push((via.unwrap_or(from), from, line));
        }

        let mut ports = Self {
            starts: vec![None; edges.len()],
            ends: vec![None; edges.len()],
        };
        for (slots, fans) in [(&mut ports.starts, leaving), (&mut ports.ends, arriving)] {
            for (node, mut lines) in fans {
                let Some(card) = nodes.get(node) else {
                    continue;
                };
                lines.sort_unstable();
                for (&(_, _, line), x) in lines.iter().zip(fan(card, lines.len())) {
                    if let Some(slot) = slots.get_mut(line) {
                        *slot = Some(x);
                    }
                }
            }
        }
        ports
    }

    fn start(&self, line: usize) -> Option<u32> {
        self.starts.get(line).copied().flatten()
    }

    fn end(&self, line: usize) -> Option<u32> {
        self.ends.get(line).copied().flatten()
    }

    /// Shift each card's arriving fan off the lines leaving the rows below,
    /// a lane at a time, so no line meets another end to end: an arriving
    /// line climbs from its run across a gap (several rows down, over open
    /// ground) into the card, past the lines leaving each row between. A
    /// card whose lines climb between the cards of its own wrapped row stays
    /// put: those gaps are only so wide.
    fn nudge(&mut self, nodes: &[Node], edges: &[Edge], courses: &[Option<Course>]) {
        let mut leaving: BTreeMap<usize, Vec<(u32, usize)>> = BTreeMap::new();
        let mut arriving: BTreeMap<usize, Vec<(u32, usize, usize)>> = BTreeMap::new();
        for (line, (edge, course)) in edges.iter().zip(courses).enumerate() {
            let (Some(course), Some(from), Some(to)) =
                (course, nodes.get(edge.from), nodes.get(edge.to))
            else {
                continue;
            };
            if let Some(start) = self.start(line) {
                leaving.entry(from.row).or_default().push((start, line));
            }
            let climbs_from = match course {
                Course::Direct => from.row,
                Course::Gutter(_) => to.row.saturating_sub(1),
            };
            if let Some(end) = self.end(line) {
                arriving
                    .entry(edge.to)
                    .or_default()
                    .push((end, line, climbs_from));
            }
        }

        let lane = i64::from(LANE);
        for (node, ends) in arriving {
            let Some(card) = nodes.get(node) else {
                continue;
            };
            let wrapped = nodes.iter().any(|other| {
                other.branch == card.branch && other.row == card.row && other.y > card.y
            });
            if wrapped {
                continue;
            }
            let clashes = |shift: i64| {
                ends.iter().any(|&(end, line, climbs_from)| {
                    leaving
                        .range(climbs_from..card.row)
                        .flat_map(|(_, starts)| starts)
                        .any(|&(start, other)| {
                            other != line
                                && i64::from(start).abs_diff(i64::from(end) + shift)
                                    < u64::from(LANE)
                        })
                })
            };
            let spread = ends
                .iter()
                .map(|&(end, _, _)| end.abs_diff(card.middle()))
                .max()
                .unwrap_or(0);
            let room = i64::from((flat(card) / 2).saturating_sub(CLEAR + spread));
            // 0, then a lane right, a lane left, two lanes right, …
            let shift = (0..=room / lane * 2)
                .map(|step| (step + 1) / 2 * lane * if step % 2 == 0 { -1 } else { 1 })
                .find(|&shift| !clashes(shift));
            if let Some(shift) = shift.filter(|&shift| shift != 0) {
                for &(end, line, _) in &ends {
                    if let Some(slot) = self.ends.get_mut(line) {
                        *slot = Some(at(end, shift));
                    }
                }
            }
        }
    }
}

/// `count` ports along a card's edge, left to right: centred, a [`LANE`]
/// apart, or closer on a card too narrow for that.
fn fan(card: &Node, count: usize) -> Vec<u32> {
    let reach = i64::from((flat(card) / 2).saturating_sub(CLEAR));
    let count = i64::try_from(count).unwrap_or(i64::MAX);
    let step = if count > 1 {
        i64::from(LANE).min(reach * 2 / (count - 1))
    } else {
        0
    };
    (0..count)
        .map(|index| at(card.middle(), (2 * index - (count - 1)) * step / 2))
        .collect()
}

/// How long the straight part of a card's top and bottom edge is: all of
/// it, but for a gate's pointed ends.
const fn flat(card: &Node) -> u32 {
    match card.shape {
        Shape::Task { .. } => card.width,
        Shape::Milestone { .. } => card.width.saturating_sub(NOTCH * 2),
    }
}

/// Where a line arriving at `x` meets a card: a task's badge, from below,
/// if the badge is over it, else the card's bottom edge.
const fn arrival(card: &Node, x: u32) -> u32 {
    let bottom = card.y + card.height;
    let off = x.abs_diff(card.middle());
    match card.shape {
        Shape::Task { .. } if off < BADGE_RADIUS => {
            bottom + (BADGE_RADIUS * BADGE_RADIUS - off * off).isqrt()
        }
        Shape::Task { .. } | Shape::Milestone { .. } => bottom,
    }
}

/// `x` moved by `offset`, clamped to the canvas.
fn at(x: u32, offset: i64) -> u32 {
    u32::try_from((i64::from(x) + offset).max(0)).unwrap_or(u32::MAX)
}

/// Each gutter line's lane: a [`LANE`] apart across its gutter, ordered by
/// where the lines arrive, so that they turn off toward their ports without
/// crossing each other.
fn lanes(courses: &[Option<Course>], ports: &Ports, frame: &Frame) -> Vec<Option<u32>> {
    let mut gutters: BTreeMap<usize, Vec<(u32, u32, usize)>> = BTreeMap::new();
    for (line, course) in courses.iter().enumerate() {
        if let Some(Course::Gutter(gutter)) = course
            && let (Some(start), Some(end)) = (ports.start(line), ports.end(line))
        {
            gutters.entry(*gutter).or_default().push((end, start, line));
        }
    }

    let mut lanes = vec![None; courses.len()];
    for (gutter, mut lines) in gutters {
        lines.sort_unstable();
        let count = i64::try_from(lines.len()).unwrap_or(i64::MAX);
        for (index, &(_, _, line)) in (0_i64..).zip(&lines) {
            if let Some(slot) = lanes.get_mut(line) {
                let offset = (2 * index - (count - 1)) * i64::from(LANE) / 2;
                *slot = Some(at(frame.gutter(gutter), offset));
            }
        }
    }
    lanes
}

/// The gap under a row.
fn under(row: usize) -> Gap {
    row.checked_sub(1).map_or(Gap::BeforeFirst, Gap::After)
}

/// Each gap's runs: one for each line that crosses it (a line straight up
/// a column has no run), and two for a line along a gutter.
fn channels(
    nodes: &[Node],
    edges: &[Edge],
    courses: &[Option<Course>],
    ports: &Ports,
    lanes: &[Option<u32>],
) -> BTreeMap<Gap, Vec<Run>> {
    let mut channels: BTreeMap<Gap, Vec<Run>> = BTreeMap::new();
    for (line, (edge, course)) in edges.iter().zip(courses).enumerate() {
        let (Some(course), Some(from), Some(to), Some(start), Some(end)) = (
            course,
            nodes.get(edge.from),
            nodes.get(edge.to),
            ports.start(line),
            ports.end(line),
        ) else {
            continue;
        };
        let first = Gap::After(from.row);
        let leave = Pin {
            x: start,
            side: Side::Below,
        };
        let arrive = Pin {
            x: end,
            side: Side::Above,
        };
        match course {
            Course::Direct if start == end => {}
            Course::Direct => channels.entry(first).or_default().push(Run {
                line,
                pins: [leave, arrive],
            }),
            Course::Gutter(_) => {
                let Some(lane) = lanes.get(line).copied().flatten() else {
                    continue;
                };
                let last = under(to.row);
                let (out, back) = if last > first {
                    (Side::Above, Side::Below)
                } else {
                    (Side::Below, Side::Above)
                };
                channels.entry(first).or_default().push(Run {
                    line,
                    pins: [leave, Pin { x: lane, side: out }],
                });
                channels.entry(last).or_default().push(Run {
                    line,
                    pins: [
                        Pin {
                            x: lane,
                            side: back,
                        },
                        arrive,
                    ],
                });
            }
        }
    }
    channels
}

/// One line's run across a gap: across between its two pins.
#[derive(Debug, Clone, Copy)]
struct Run {
    line: usize,
    pins: [Pin; 2],
}

impl Run {
    fn left(&self) -> u32 {
        self.pins.iter().map(|pin| pin.x).min().unwrap_or(0)
    }

    fn right(&self) -> u32 {
        self.pins.iter().map(|pin| pin.x).max().unwrap_or(0)
    }
}

/// Where a run turns out of its gap: at `x`, toward one side.
#[derive(Debug, Clone, Copy)]
struct Pin {
    x: u32,
    side: Side,
}

/// Which way a run leaves its gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Up, into a card in the row above, or up a gutter.
    Above,

    /// Down, from a card in the row below, or down a gutter.
    Below,
}

/// Which track each run across a gap takes, by line, counted from the top;
/// and how many tracks there are.
#[derive(Debug)]
struct Tracks {
    of: BTreeMap<usize, usize>,
    count: usize,
}

impl Tracks {
    /// Give each run a track, top down, by the constrained left-edge
    /// algorithm. A run spanning a line that drops out of the gap must pass
    /// above it, and one spanning a line that climbs out, below it, or they
    /// cross; two lines meeting end to end must, or they'd overlap. Each
    /// track takes every run whose runs above are placed, left to right,
    /// leaving room between them. If the rest wait on each other, the run
    /// held up by the fewest overlaps, then crossings, goes next.
    fn assign(runs: &[Run]) -> Self {
        let mut above: Vec<Vec<(usize, bool)>> = vec![Vec::new(); runs.len()];
        for (index, run) in runs.iter().enumerate() {
            for (other_index, other) in runs.iter().enumerate() {
                if other_index == index {
                    continue;
                }
                for pin in run.pins {
                    let end_to_end = other.pins.iter().find(|end| end.x.abs_diff(pin.x) < LANE);
                    let (higher, lower, must) = match (end_to_end, pin.side) {
                        (Some(end), Side::Below) if end.side == Side::Above => {
                            (other_index, index, true)
                        }
                        (Some(_), _) => continue,
                        (None, _) if !(other.left() < pin.x && pin.x < other.right()) => continue,
                        (None, Side::Below) => (other_index, index, false),
                        (None, Side::Above) => (index, other_index, false),
                    };
                    if let Some(above) = above.get_mut(lower) {
                        above.push((higher, must));
                    }
                }
            }
        }

        let mut track_of: Vec<Option<usize>> = vec![None; runs.len()];
        let mut count = 0;
        while track_of.iter().any(Option::is_none) {
            let unplaced = |run: &usize| track_of.get(*run).copied().flatten().is_none();
            let held = |run: usize, musts_only: bool| {
                above
                    .get(run)
                    .into_iter()
                    .flatten()
                    .filter(|&&(higher, must)| (must || !musts_only) && unplaced(&higher))
                    .count()
            };
            let mut ready: Vec<usize> = (0..runs.len())
                .filter(unplaced)
                .filter(|&run| held(run, false) == 0)
                .collect();
            if ready.is_empty() {
                ready.extend(
                    (0..runs.len())
                        .filter(unplaced)
                        .min_by_key(|&run| (held(run, true), held(run, false))),
                );
            }
            ready.sort_by_key(|&run| runs.get(run).map_or(0, Run::left));

            let mut reach: Option<u32> = None;
            for index in ready {
                let Some(run) = runs.get(index) else { continue };
                if reach.is_none_or(|reach| run.left() >= reach + LANE * 2)
                    && let Some(slot) = track_of.get_mut(index)
                {
                    *slot = Some(count);
                    reach = Some(run.right());
                }
            }
            count += 1;
        }

        Self {
            of: runs
                .iter()
                .zip(track_of)
                .filter_map(|(run, track)| Some((run.line, track?)))
                .collect(),
            count,
        }
    }

    /// The height of `line`'s track: the tracks spread evenly down the gap
    /// from `top` to `bottom`.
    fn y(&self, line: usize, (top, bottom): (u32, u32)) -> Option<u32> {
        let track = u32::try_from(*self.of.get(&line)?).ok()?;
        let count = u32::try_from(self.count).ok()?;
        Some(top + bottom.saturating_sub(top) * (track + 1) / (count + 1))
    }
}

/// Make room for each gap's tracks, a [`TRACK`] apart: push everything below
/// a crowded gap down, and redraw the bands.
fn grow(frame: &mut Frame, nodes: &mut [Node], tracks: &BTreeMap<Gap, Tracks>, tiers: &Tiers) {
    let extra = |gap: Gap| {
        let count = tracks.get(&gap).map_or(0, |tracks| tracks.count);
        let (top, bottom) = frame.bounds(gap);
        u32::try_from(count + 1)
            .unwrap_or(u32::MAX)
            .saturating_mul(TRACK)
            .saturating_sub(bottom.saturating_sub(top))
    };
    // Rows count from the bottom, so a row moves down by every gap above it
    let mut shifts = vec![0; frame.rows.len()];
    let mut shift = 0;
    for (row, slot) in shifts.iter_mut().enumerate().rev() {
        shift += extra(Gap::After(row));
        *slot = shift;
    }
    let total = shift + extra(Gap::BeforeFirst);
    if total == 0 {
        return;
    }

    for (row, shift) in frame.rows.iter_mut().zip(&shifts) {
        row.0 += shift;
    }
    for node in nodes.iter_mut() {
        node.y += shifts.get(node.row).copied().unwrap_or(0);
    }
    frame.panel_bottom += total;
    frame.height += total;
    for branch in &mut frame.branches {
        branch.height += total;
    }
    let bands = bands(frame, nodes, tiers);
    frame.bands = bands;
}
