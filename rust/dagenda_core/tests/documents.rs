//! Plans made of several documents: a public plan and a private one that
//! extends it.

use dagenda_core::{
    id::Id,
    parser,
    plan::{DocumentId, Plan, Readiness, ResolveErrorKind, warning::WarningKind},
    text::Text,
};
use testresult::TestResult;

const PUBLIC: &str = "roadmap
title The public plan

track automerge [Automerge]
  [ ] signatures [Signatures]
  [x] fragments [Fragments]

milestone repo3 [Repo 3]
  signatures
";

const PRIVATE: &str = "roadmap
title The team's plan

extend signatures
  dri @alice
  needs fragments
  [ ] wireFormat [Pick the wire format]
    est 2026-12-01
";

fn resolve(sources: &[&str]) -> Result<Plan, Box<dyn std::error::Error>> {
    let documents = sources
        .iter()
        .copied()
        .map(parser::document)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Plan::resolve(&documents)?)
}

fn id(id: &str) -> Result<Id, Box<dyn std::error::Error>> {
    Ok(Id::new(id)?)
}

#[test]
fn extend_adds_details_and_tasks_from_another_document() -> TestResult {
    let plan = resolve(&[PUBLIC, PRIVATE])?;
    assert!(plan.warnings().is_empty(), "{:?}", plan.warnings());

    let signatures = plan.task(&id("signatures")?).ok_or("no signatures")?;
    assert_eq!(
        signatures
            .dri
            .as_ref()
            .map(|dri| (dri.node.to_string(), dri.at.document)),
        Some((String::from("@alice"), DocumentId::new(1)))
    );
    let needs: Vec<_> = signatures.needs.iter().map(|need| &need.node).collect();
    assert_eq!(
        needs,
        [&id("fragments")?, &id("wireFormat")?],
        "a task written under an `extend` is one it needs"
    );

    let wire_format = plan.task(&id("wireFormat")?).ok_or("no wireFormat")?;
    assert_eq!(
        wire_format.track,
        Some(id("automerge")?),
        "and lives in its track"
    );
    assert_eq!(wire_format.defined.document, DocumentId::new(1));

    assert_eq!(
        plan.readiness(&id("signatures")?),
        Some(Readiness::Blocked(vec![id("wireFormat")?])),
        "fragments, from the private plan's `needs`, is done; wireFormat isn't"
    );
    Ok(())
}

#[test]
fn each_document_keeps_its_title() -> TestResult {
    let plan = resolve(&[PUBLIC, PRIVATE])?;
    let titles: Vec<_> = (0..2)
        .map(|index| plan.title(DocumentId::new(index)).map(Text::as_str))
        .collect();
    assert_eq!(titles, [Some("The public plan"), Some("The team's plan")]);
    Ok(())
}

#[test]
fn the_public_plan_resolves_alone() -> TestResult {
    let plan = resolve(&[PUBLIC])?;
    let signatures = plan.task(&id("signatures")?).ok_or("no signatures")?;
    assert!(signatures.dri.is_none());
    assert!(
        signatures.needs.is_empty(),
        "the private plan's needs aren't here"
    );
    Ok(())
}

#[test]
fn an_id_defined_in_two_documents_points_at_both() -> TestResult {
    let documents = [
        parser::document(PUBLIC)?,
        parser::document("roadmap\n[ ] fragments\n")?,
    ];
    let errors = Plan::resolve(&documents).err().ok_or("resolved")?;
    assert_eq!(errors.len().get(), 1);
    let error = errors.first();
    assert_eq!(error.location().document, DocumentId::new(1));
    let ResolveErrorKind::Duplicate { first, .. } = error.kind() else {
        return Err(format!("expected a duplicate, got {error}").into());
    };
    assert_eq!(first.document, DocumentId::new(0));
    Ok(())
}

#[test]
fn two_documents_cant_both_set_a_detail() -> TestResult {
    let documents = [
        parser::document(PUBLIC)?,
        parser::document(PRIVATE)?,
        parser::document("roadmap\nextend signatures\n  dri @bob\n")?,
    ];
    let errors = Plan::resolve(&documents).err().ok_or("resolved")?;
    let error = errors.first();
    assert_eq!(
        error.to_string(),
        "`dri` is already set for `signatures`; each detail has one owner"
    );
    assert_eq!(error.location().document, DocumentId::new(2));
    Ok(())
}

#[test]
fn extending_a_task_in_its_own_document_is_a_warning() -> TestResult {
    let plan = resolve(&["roadmap\n[ ] a\nextend a\n  est 2026-12\n"])?;
    let kinds: Vec<_> = plan.warnings().iter().map(|w| &w.kind).collect();
    assert_eq!(kinds, [&WarningKind::ExtendSameDocument(id("a")?)]);
    Ok(())
}
