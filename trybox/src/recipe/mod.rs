//! Recipes: what a project is, its hello world, its tour, and what it needs from the machine.
//! One module per recipe; `matrix` holds the requirement helpers, `render` the man pages.

use crate::backend::Backend;
use serde::Serialize;
use speccheck::Requirements;

mod bare;
mod matrix;
mod mlx;
mod render;
mod torch;
mod whisper;

pub use render::{check_here, man, outcome_here, requirements_section, table, wrap};

/// One thing to try: a title, why it matters, the command, what you should see.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Example {
    pub title: &'static str,
    /// What this step shows, in plain words. Mention downloads and durations here.
    pub learn: &'static str,
    /// Shell command, valid inside the sandbox.
    pub run: &'static str,
    /// What success looks like.
    pub expect: &'static str,
    /// Hugging Face repos this step needs. trybox fetches them before the command runs, as a
    /// visible setup phase, and then runs the command with download progress bars off, so
    /// nothing about fetching interleaves with the step's own output.
    pub models: &'static [&'static str],
}

/// A recipe is a guided exploration of one project: what it is, a hello world,
/// a tour from easy to advanced, and what the resident agent should propose.
/// Environment details (backend, packages) are here too, but the user never
/// needs to look at them.
#[derive(Debug, Clone, Serialize)]
pub struct Recipe {
    pub name: &'static str,
    pub summary: &'static str,
    /// A short paragraph: what the project is, who makes it, why you would care.
    pub what: &'static str,
    pub repo: &'static str,
    pub backend: Backend,
    pub packages: &'static [&'static str],
    pub python: &'static str,
    pub hello: Example,
    /// Ordered from first steps to showing off what the project can do.
    pub tour: &'static [Example],
    /// Open-ended experiments the agent proposes once the tour is done.
    pub suggestions: &'static [&'static str],
    pub caveats: &'static [&'static str],
    /// What the machine needs, as a speccheck requirements matrix.
    #[serde(skip)]
    pub requirements: fn() -> Requirements,
}

/// Every recipe, in menu order.
pub const RECIPES: &[Recipe] = &[mlx::RECIPE, whisper::RECIPE, torch::RECIPE, bare::RECIPE];

pub fn recipe(name: &str) -> Result<&'static Recipe, String> {
    RECIPES.iter().find(|r| r.name == name).ok_or_else(|| {
        let names: Vec<_> = RECIPES.iter().map(|r| r.name).collect();
        format!("unknown recipe {name:?}; known: {}", names.join(", "))
    })
}
