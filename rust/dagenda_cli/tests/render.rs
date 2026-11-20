//! `dagenda render`: which format a path asks for, and what each produces.

use dagenda_cli::render::Format;
use dagenda_core::check::{Source, check};
use std::path::Path;
use testresult::TestResult;

const PLAN: &str = "roadmap
title Rendering
track core [Core]
  [x] task parser [Parser]
  [ ] task resolver [Resolver]
    needs parser
";

#[test]
fn the_extension_picks_the_format() {
    assert_eq!(
        Format::for_path(Path::new("out/plan.svg")),
        Some(Format::Svg)
    );
    assert_eq!(Format::for_path(Path::new("plan.txt")), Some(Format::Text));
    assert_eq!(Format::for_path(Path::new("plan")), None);
    assert_eq!(Format::default(), Format::Text);
}

#[test]
fn each_format_draws_the_whole_plan() -> TestResult {
    let (plan, problems) = check(&[Source {
        name: "plan.dagenda".into(),
        text: PLAN.into(),
    }]);
    assert!(problems.is_empty(), "{problems:?}");
    let plan = plan.ok_or("no plan")?;

    let text = Format::Text.render(&plan);
    assert!(text.starts_with("Rendering\n"));
    assert!(text.contains("[ ] resolver · Resolver"));
    assert!(text.ends_with('\n'));

    let svg = Format::Svg.render(&plan);
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains(r#"data-id="resolver""#));
    assert!(svg.ends_with("</svg>\n"));
    Ok(())
}
