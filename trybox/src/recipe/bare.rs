//! bare: An empty Python environment to explore anything not covered by a recipe

use super::matrix::*;
use super::{Example, Recipe};
use crate::backend::Backend;
use speccheck::{Requirements, Severity};

pub const RECIPE: Recipe = Recipe {
    name: "bare",
    summary: "An empty Python environment to explore anything not covered by a recipe",
    what: "A clean Python with nothing installed. Use it when there is no recipe for the project you want to try: install it with `uv pip install`, poke at it, throw the sandbox away.",
    repo: "https://docs.astral.sh/uv/",
    backend: Backend::Uv,
    packages: &[],
    python: "3.12",
    hello: Example {
        title: "Which Python is this",
        learn: "Confirms the sandbox has its own interpreter, separate from the system one.",
        run: "python -c 'import sys, platform; print(sys.version.split()[0], platform.machine(), sys.prefix)'",
        expect: "A version, the architecture, and a prefix inside the sandbox directory.",
        models: &[],
    },
    tour: &[
        Example {
            title: "Install something and see what came with it",
            learn: "Packages install into the sandbox only. Swap `rich` for whatever you want to explore.",
            run: "uv pip install rich && python -c 'from rich import print; print(\"[bold green]hello from the sandbox[/]\")'",
            expect: "A short install log, then green bold text.",
            models: &[],
        },
        Example {
            title: "List what is installed",
            learn: "Shows the sandbox is self-contained; nothing here is on your machine.",
            run: "uv pip list",
            expect: "A short table of packages.",
            models: &[],
        },
    ],
    suggestions: &[
        "Install the project the user names, run its own hello world, and report what it pulled in.",
    ],
    caveats: &[],
    requirements,
};

fn requirements() -> Requirements {
    Requirements {
        version: speccheck::CONTRACT_VERSION,
        subject: "bare".into(),
        rules: vec![
            rule(tool("uv"), Severity::Blocks, "uv builds the sandbox"),
            rule(disk(1), Severity::Blocks, "room for an interpreter"),
        ],
        tiers: vec![],
    }
}
