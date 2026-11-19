# dagenda

> [!WARNING]
> Nothing works yet. This is the scaffold: the toolchain, the checks, and an empty core crate.

Roadmaps as dependency graphs, for the Ink & Switch Department of Data: Automerge, Subduction, Keyhive, and their neighbours. The world should be able to see the milestones and the features that lead to them. The day-to-day tasks, who's on them, and the notes stay with the team.

## Planned

- _One graph, three views._ A nodes-and-edges tech tree, a Gantt chart, and a skill tree in the spirit of Final Fantasy X's Sphere Grid.
- _Text and picture in step._ A plain-text notation, and a WYSIWYG editor over the same source: change either and the other follows, as in [rain].
- _A public site and a collaborative editor._ The site is static. The editor keeps its documents in Automerge, synced over [Subduction].
- _Links to the work._ Nodes refer to GitHub issues and pull requests, resolved by [sans-effort] routines.
- _Access control with [Keyhive]_, later.

## Layout

```
rust/
└── dagenda_core/   the roadmap model (no_std); empty for now
```

## Development

```console
$ nix develop            # toolchain and a command menu
$ cargo test --workspace
$ nix run .#ci-clippy    # also: ci-deny, ci-doc, ci-fmt, ci-msrv, ci-mutants, ci-no-std, ci-test, ci-typos
```

Every CI job is one of these flake apps, so a local run checks exactly what CI does.

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

[Keyhive]: https://github.com/inkandswitch/keyhive
[rain]: https://github.com/inkandswitch/rain
[sans-effort]: https://github.com/inkandswitch/sans-effort
[Subduction]: https://github.com/inkandswitch/subduction
