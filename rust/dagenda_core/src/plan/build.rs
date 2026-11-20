//! Resolution, in passes: define every id, merge `extend`s, check references,
//! order the work, then derive what's finished and what to warn about.
//!
//! The first three passes keep going past an error, collecting every one,
//! so a plan's mistakes are all reported at once. Ordering only runs on a
//! plan without them: a loop is only worth reporting once every id it
//! passes through is known.

use super::{
    DocumentId, Entry, Link, Located, Location, Milestone, Node, NodeKind, Plan, ResolveError,
    ResolveErrorKind, Site, Task, Track, flat,
    warning::{Warning, WarningKind},
};
use crate::{ast, errors::Errors, id::Id, span::Spanned, status::Status, text::Text};
use alloc::{
    collections::{BTreeMap, VecDeque},
    vec,
    vec::Vec,
};

/// A plan under construction.
#[derive(Debug, Default)]
pub(super) struct Builder<'d> {
    titles: Vec<Option<Located<Text>>>,
    tracks: Vec<Track>,
    tasks: Vec<Task>,
    milestones: Vec<Milestone>,
    index: BTreeMap<Id, Entry>,

    /// `extend` blocks, merged once every document's ids are known.
    extends: Vec<(DocumentId, &'d Spanned<ast::Extend>)>,

    /// Tasks written under an `extend`, and the task each is under, so they
    /// can take its track once every document's tracks are known.
    under: Vec<(Id, Id)>,

    warnings: Vec<Warning>,

    /// Everything wrong so far.
    errors: Vec<ResolveError>,
}

impl<'d> Builder<'d> {
    /// Define everything in one document.
    pub(super) fn document(&mut self, document: DocumentId, ast: &'d ast::Document) {
        self.titles
            .push(ast.title.as_ref().map(|title| locate(document, title)));

        for item in &ast.items {
            match item {
                ast::Item::Track(track) => self.track(document, track, None),
                ast::Item::Task(task) => self.task(document, task, None, None),
                ast::Item::Milestone(milestone) => self.milestone(document, milestone),
                ast::Item::Extend(extend) => {
                    for under in &extend.node.under {
                        self.task(document, under, None, Some(&extend.node.id.node));
                    }
                    self.extends.push((document, extend));
                }
            }
        }
    }

    /// Everything wrong with the plan, in source order, if anything is.
    pub(super) fn failed(&mut self) -> Option<Errors<ResolveError>> {
        let mut errors = core::mem::take(&mut self.errors);
        errors.sort_by_key(|error| {
            let at = error.location();
            (at.document.index(), at.span.start)
        });
        Errors::new(errors)
    }

    /// Index `id`. A second definition is an error, and stays out of the
    /// index, but is still built, so what's wrong inside it is found too.
    fn define(&mut self, id: &Spanned<Id>, at: Location, node: Node) {
        if let Some(first) = self.index.get(&id.node) {
            self.errors.push(ResolveError::new(
                Location {
                    document: at.document,
                    span: id.span,
                },
                ResolveErrorKind::Duplicate {
                    id: id.node.clone(),
                    first: first.at,
                },
            ));
            return;
        }
        self.index.insert(id.node.clone(), Entry { node, at });
    }

    fn track(&mut self, document: DocumentId, track: &'d Spanned<ast::Track>, parent: Option<&Id>) {
        let ast = &track.node;
        let defined = Location {
            document,
            span: track.span,
        };
        self.define(&ast.id, defined, Node::Track(self.tracks.len()));
        self.tracks.push(Track {
            id: ast.id.node.clone(),
            label: ast.label.as_ref().map(|label| label.node.clone()),
            parent: parent.cloned(),
            dri: ast.dri.as_ref().map(|dri| locate(document, dri)),
            notes: ast
                .notes
                .iter()
                .map(|note| locate(document, note))
                .collect(),
            defined,
        });

        for child in &ast.children {
            match child {
                ast::TrackChild::Track(nested) => {
                    self.track(document, nested, Some(&ast.id.node));
                }
                ast::TrackChild::Task(task) => {
                    self.task(document, task, Some(&ast.id.node), None);
                }
            }
        }
    }

    fn task(
        &mut self,
        document: DocumentId,
        task: &'d Spanned<ast::Task>,
        track: Option<&Id>,
        outer: Option<&Id>,
    ) {
        let ast = &task.node;
        let details = &ast.details;
        let defined = Location {
            document,
            span: task.span,
        };
        self.define(&ast.id, defined, Node::Task(self.tasks.len()));
        if let (None, Some(outer)) = (track, outer) {
            self.under.push((ast.id.node.clone(), outer.clone()));
        }
        self.tasks.push(Task {
            id: ast.id.node.clone(),
            label: ast.label.as_ref().map(|label| label.node.clone()),
            status: ast.status.node,
            track: track.cloned(),
            needs: details
                .needs
                .iter()
                .chain(ast.under.iter().map(|under| &under.node.id))
                .map(|need| locate(document, need))
                .collect(),
            est: details.est.as_ref().map(|est| locate(document, est)),
            target: details
                .target
                .as_ref()
                .map(|target| locate(document, target)),
            dri: details.dri.as_ref().map(|dri| locate(document, dri)),
            refs: details.refs.iter().map(|r| locate(document, r)).collect(),
            links: details
                .links
                .iter()
                .map(|link| locate(document, link))
                .collect(),
            notes: details
                .notes
                .iter()
                .map(|note| locate(document, note))
                .collect(),
            defined,
            status_at: Location {
                document,
                span: ast.status.span,
            },
        });

        for under in &ast.under {
            self.task(document, under, track, Some(&ast.id.node));
        }
    }

    fn milestone(&mut self, document: DocumentId, milestone: &'d Spanned<ast::Milestone>) {
        let ast = &milestone.node;
        let defined = Location {
            document,
            span: milestone.span,
        };
        self.define(&ast.id, defined, Node::Milestone(self.milestones.len()));
        self.milestones.push(Milestone {
            id: ast.id.node.clone(),
            label: ast.label.as_ref().map(|label| label.node.clone()),
            target: ast.target.as_ref().map(|target| locate(document, target)),
            notes: ast
                .notes
                .iter()
                .map(|note| locate(document, note))
                .collect(),
            needs: ast
                .needs
                .iter()
                .map(|member| locate(document, member))
                .collect(),
            wants: ast
                .wants
                .iter()
                .map(|member| locate(document, member))
                .collect(),
            defined,
        });
    }

    /// Merge every `extend` into the task it names.
    pub(super) fn extends(&mut self) {
        for (document, extend) in core::mem::take(&mut self.extends) {
            let ast = &extend.node;
            let here = Location {
                document,
                span: ast.id.span,
            };

            let index = match self.index.get(&ast.id.node).map(|entry| entry.node) {
                Some(Node::Task(index)) => index,
                Some(node @ (Node::Track(_) | Node::Milestone(_))) => {
                    self.errors.push(ResolveError::new(
                        here,
                        ResolveErrorKind::WrongKind {
                            id: ast.id.node.clone(),
                            is: node.kind(),
                            site: Site::Extend,
                        },
                    ));
                    continue;
                }
                None => {
                    self.errors.push(ResolveError::new(
                        here,
                        ResolveErrorKind::Unknown(ast.id.node.clone()),
                    ));
                    continue;
                }
            };

            let Some(task) = self.tasks.get_mut(index) else {
                continue;
            };
            if task.defined.document == document {
                self.warnings.push(Warning {
                    at: here,
                    kind: WarningKind::ExtendSameDocument(ast.id.node.clone()),
                });
            }
            merge(task, document, &ast.details, &mut self.errors);
            task.needs.extend(
                ast.under
                    .iter()
                    .map(|under| locate(document, &under.node.id)),
            );
        }
        self.inherit_tracks();
    }

    /// Give each task written under an `extend` the track of the task it's
    /// under, or of the one that's under, and so on.
    fn inherit_tracks(&mut self) {
        let under: BTreeMap<Id, Id> = core::mem::take(&mut self.under).into_iter().collect();
        for index in 0..self.tasks.len() {
            let Some(task) = self.tasks.get(index) else {
                continue;
            };
            if task.track.is_some() {
                continue;
            }

            let mut track = None;
            let mut next = under.get(&task.id);
            for _ in 0..self.tasks.len() {
                let Some(outer) = next else { break };
                let Some(Entry {
                    node: Node::Task(outer),
                    ..
                }) = self.index.get(outer)
                else {
                    break;
                };
                let Some(outer) = self.tasks.get(*outer) else {
                    break;
                };
                if outer.track.is_some() {
                    track.clone_from(&outer.track);
                    break;
                }
                next = under.get(&outer.id);
            }

            if let Some(task) = self.tasks.get_mut(index) {
                task.track = track;
            }
        }
    }

    /// Check that every `needs` and milestone member names a task or a
    /// milestone.
    pub(super) fn references(&mut self) {
        let needs = self
            .tasks
            .iter()
            .flat_map(|task| &task.needs)
            .map(|need| self.check(need, Site::Needs));
        let members = self
            .milestones
            .iter()
            .flat_map(|milestone| milestone.needs.iter().chain(&milestone.wants))
            .map(|member| self.check(member, Site::Member));
        let wrong: Vec<ResolveError> = needs.chain(members).filter_map(Result::err).collect();
        self.errors.extend(wrong);
    }

    fn check(&self, reference: &Located<Id>, site: Site) -> Result<(), ResolveError> {
        match self.index.get(&reference.node).map(|entry| entry.node) {
            Some(Node::Task(_) | Node::Milestone(_)) => Ok(()),
            Some(Node::Track(_)) => Err(ResolveError::new(
                reference.at,
                ResolveErrorKind::WrongKind {
                    id: reference.node.clone(),
                    is: NodeKind::Track,
                    site,
                },
            )),
            None => Err(ResolveError::new(
                reference.at,
                ResolveErrorKind::Unknown(reference.node.clone()),
            )),
        }
    }

    /// What each task, then each milestone, waits on: a task on its `needs`,
    /// a milestone on its must-haves.
    pub(super) fn graph(&self) -> Vec<Vec<Edge>> {
        let edges = |references: &[Located<Id>]| -> Vec<Edge> {
            references
                .iter()
                .filter_map(|reference| {
                    flat(&self.index, self.tasks.len(), &reference.node).map(|to| Edge {
                        to,
                        at: reference.at,
                    })
                })
                .collect()
        };

        self.tasks
            .iter()
            .map(|task| edges(&task.needs))
            .chain(
                self.milestones
                    .iter()
                    .map(|milestone| edges(&milestone.needs)),
            )
            .collect()
    }

    /// Every task and milestone, each after everything it waits on.
    pub(super) fn order(&self, graph: &[Vec<Edge>]) -> Result<Vec<usize>, ResolveError> {
        let mut waiting: Vec<usize> = graph.iter().map(Vec::len).collect();
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); graph.len()];
        for (from, edges) in graph.iter().enumerate() {
            for edge in edges {
                if let Some(dependents) = dependents.get_mut(edge.to) {
                    dependents.push(from);
                }
            }
        }

        let mut ready: VecDeque<usize> = waiting
            .iter()
            .enumerate()
            .filter(|(_, waiting)| **waiting == 0)
            .map(|(node, _)| node)
            .collect();

        let mut order = Vec::with_capacity(graph.len());
        while let Some(node) = ready.pop_front() {
            order.push(node);
            for &dependent in dependents.get(node).into_iter().flatten() {
                if let Some(count) = waiting.get_mut(dependent) {
                    *count -= 1;
                    if *count == 0 {
                        ready.push_back(dependent);
                    }
                }
            }
        }

        if order.len() == graph.len() {
            Ok(order)
        } else {
            Err(self.cycle(graph, &waiting))
        }
    }

    /// Find a cycle among the nodes `order` couldn't place. Each of them
    /// waits on another, so following those edges must come back around.
    fn cycle(&self, graph: &[Vec<Edge>], waiting: &[usize]) -> ResolveError {
        let stuck = |node: usize| waiting.get(node).is_some_and(|count| *count > 0);
        let mut node = (0..graph.len()).find(|&node| stuck(node)).unwrap_or(0);
        let mut path: Vec<(usize, Location)> = Vec::new();

        while path.len() <= graph.len() {
            if let Some(start) = path.iter().position(|(seen, _)| *seen == node) {
                let cycle = path.get(start..).unwrap_or_default();
                let ids = cycle
                    .iter()
                    .map(|(seen, _)| *seen)
                    .chain([node])
                    .filter_map(|seen| self.id_of(seen))
                    .collect();
                let at = cycle.first().map_or(self.location_of(node), |(_, at)| *at);
                return ResolveError::new(at, ResolveErrorKind::Cycle(ids));
            }

            let Some(edge) = graph
                .get(node)
                .and_then(|edges| edges.iter().find(|edge| stuck(edge.to)))
            else {
                break;
            };
            path.push((node, edge.at));
            node = edge.to;
        }

        let ids = path
            .iter()
            .filter_map(|(seen, _)| self.id_of(*seen))
            .collect();
        ResolveError::new(self.location_of(node), ResolveErrorKind::Cycle(ids))
    }

    fn id_of(&self, node: usize) -> Option<Id> {
        self.tasks
            .get(node)
            .map(|task| task.id.clone())
            .or_else(|| {
                node.checked_sub(self.tasks.len())
                    .and_then(|milestone| self.milestones.get(milestone))
                    .map(|milestone| milestone.id.clone())
            })
    }

    fn location_of(&self, node: usize) -> Location {
        self.tasks
            .get(node)
            .map(|task| task.defined)
            .or_else(|| {
                node.checked_sub(self.tasks.len())
                    .and_then(|milestone| self.milestones.get(milestone))
                    .map(|milestone| milestone.defined)
            })
            .unwrap_or_default()
    }

    /// Derive what's finished, the critical chains, and the warnings.
    pub(super) fn finish(mut self, graph: &[Vec<Edge>], order: &[usize]) -> Plan {
        let tasks = self.tasks.len();
        let mut finished: Vec<bool> = self
            .tasks
            .iter()
            .map(|task| task.status.is_finished())
            .chain(self.milestones.iter().map(|_| false))
            .collect();
        let mut chain = vec![Link::default(); graph.len()];

        for &node in order {
            let edges = graph.get(node).map_or(&[][..], Vec::as_slice);
            let done = |to: usize| finished.get(to).copied().unwrap_or(false);

            if node >= tasks {
                let reached = edges.iter().all(|edge| done(edge.to));
                if let Some(finished) = finished.get_mut(node) {
                    *finished = reached;
                }
            }

            if !finished.get(node).copied().unwrap_or(false) {
                let mut longest = Link::default();
                for edge in edges {
                    let length = chain.get(edge.to).map_or(0, |link| link.length);
                    if length > longest.length {
                        longest = Link {
                            length,
                            previous: Some(edge.to),
                        };
                    }
                }
                if node < tasks {
                    longest.length += 1;
                }
                if let Some(link) = chain.get_mut(node) {
                    *link = longest;
                }
            }
        }

        let warnings = self.warn(&finished);
        self.warnings.extend(warnings);
        self.warnings.sort_by_key(|warning| warning.at);

        Plan {
            titles: self.titles,
            tracks: self.tracks,
            tasks: self.tasks,
            milestones: self.milestones,
            index: self.index,
            finished,
            chain,
            warnings: self.warnings,
        }
    }

    fn warn(&self, finished: &[bool]) -> Vec<Warning> {
        let task = |id: &Id| match self.index.get(id).map(|entry| entry.node) {
            Some(Node::Task(index)) => self.tasks.get(index),
            Some(Node::Track(_) | Node::Milestone(_)) | None => None,
        };
        let is_finished = |id: &Id| {
            flat(&self.index, self.tasks.len(), id)
                .and_then(|node| finished.get(node).copied())
                .unwrap_or(false)
        };
        let has = |id: &Id, status: Status| task(id).is_some_and(|task| task.status == status);

        let mut warnings = Vec::new();
        for milestone in &self.milestones {
            if milestone.needs.is_empty() {
                warnings.push(Warning {
                    at: milestone.defined,
                    kind: WarningKind::EmptyMilestone(milestone.id.clone()),
                });
            }
            for member in &milestone.needs {
                let kind = if has(&member.node, Status::Dropped) {
                    WarningKind::DroppedMustHave {
                        milestone: milestone.id.clone(),
                        task: member.node.clone(),
                    }
                } else if has(&member.node, Status::Deferred) {
                    WarningKind::DeferredMustHave {
                        milestone: milestone.id.clone(),
                        task: member.node.clone(),
                    }
                } else {
                    continue;
                };
                warnings.push(Warning {
                    at: member.at,
                    kind,
                });
            }
        }

        for current in &self.tasks {
            for need in &current.needs {
                let kind = if current.status == Status::Done && !is_finished(&need.node) {
                    WarningKind::DoneBeforeNeeds {
                        task: current.id.clone(),
                        need: need.node.clone(),
                    }
                } else if current.status != Status::Dropped && has(&need.node, Status::Dropped) {
                    WarningKind::NeedsDropped {
                        task: current.id.clone(),
                        need: need.node.clone(),
                    }
                } else {
                    continue;
                };
                warnings.push(Warning { at: need.at, kind });
            }
        }
        warnings
    }
}

/// One node waiting on another, because of the reference at `at`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Edge {
    to: usize,
    at: Location,
}

/// Merge an `extend`'s details into its task.
/// A detail another document already set is an error, added to `errors`.
fn merge(
    task: &mut Task,
    document: DocumentId,
    details: &ast::Details,
    errors: &mut Vec<ResolveError>,
) {
    task.needs
        .extend(details.needs.iter().map(|need| locate(document, need)));
    task.refs
        .extend(details.refs.iter().map(|r| locate(document, r)));
    task.links
        .extend(details.links.iter().map(|link| locate(document, link)));
    task.notes
        .extend(details.notes.iter().map(|note| locate(document, note)));
    errors.extend(
        set_once(
            &mut task.est,
            details.est.as_ref(),
            "est",
            &task.id,
            document,
        )
        .err(),
    );
    errors.extend(
        set_once(
            &mut task.target,
            details.target.as_ref(),
            "target",
            &task.id,
            document,
        )
        .err(),
    );
    errors.extend(
        set_once(
            &mut task.dri,
            details.dri.as_ref(),
            "dri",
            &task.id,
            document,
        )
        .err(),
    );
}

/// Fill a detail that only one document can set.
fn set_once<T: Clone>(
    slot: &mut Option<Located<T>>,
    value: Option<&Spanned<T>>,
    key: &'static str,
    id: &Id,
    document: DocumentId,
) -> Result<(), ResolveError> {
    let Some(value) = value else {
        return Ok(());
    };
    if let Some(first) = slot {
        return Err(ResolveError::new(
            Location {
                document,
                span: value.span,
            },
            ResolveErrorKind::Conflict {
                id: id.clone(),
                key,
                first: first.at,
            },
        ));
    }
    *slot = Some(locate(document, value));
    Ok(())
}

fn locate<T: Clone>(document: DocumentId, spanned: &Spanned<T>) -> Located<T> {
    Located {
        node: spanned.node.clone(),
        at: Location {
            document,
            span: spanned.span,
        },
    }
}
