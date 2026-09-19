//! grindreport: read a grindstone dataset (Parquet facts + manifest) and
//! render a usage report for a person or an agent.
//!
//! One deterministic operation: `load` -> `analyze` -> `Report`, then any of
//! the renderers in `render`. The `Report` struct serialized as JSON is the
//! machine contract (`REPORT_VERSION`).

pub mod analyze;
pub mod model;
pub mod pricing;
pub mod read;
pub mod render;
pub mod util;

pub use analyze::{analyze, Options, Report, REPORT_VERSION};
pub use model::{Dataset, Manifest};
pub use pricing::Pricing;
pub use read::{load, load_manifest, Filter};

/// Output formats a report can be rendered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Markdown,
    Html,
    Json,
}

impl Format {
    pub fn render(self, r: &Report) -> String {
        match self {
            Format::Text => render::text::render(r),
            Format::Markdown => render::markdown::render(r),
            Format::Html => render::html::render(r),
            Format::Json => serde_json::to_string_pretty(r).expect("report serializes") + "\n",
        }
    }
}
