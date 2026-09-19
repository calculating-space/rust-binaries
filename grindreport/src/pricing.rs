//! Optional price table (spec §5): prices are the user's input, never a
//! fact. USD per million tokens, keyed by model id; a key matches a model
//! exactly or as a prefix (longest prefix wins).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Pricing {
    pub version: String,
    pub currency: String,
    pub models: BTreeMap<String, ModelPrice>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
}

impl Pricing {
    pub fn load(path: &Path) -> Result<Pricing> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading pricing {}", path.display()))?;
        let mut p: Pricing = if path.extension().is_some_and(|e| e == "json") {
            serde_json::from_str(&text).context("parsing pricing json")?
        } else {
            toml::from_str(&text).context("parsing pricing toml")?
        };
        if p.currency.is_empty() {
            p.currency = "USD".into();
        }
        if p.version.is_empty() {
            p.version = "unversioned".into();
        }
        Ok(p)
    }

    pub fn price_for(&self, model: &str) -> Option<&ModelPrice> {
        if let Some(p) = self.models.get(model) {
            return Some(p);
        }
        self.models
            .iter()
            .filter(|(k, _)| model.starts_with(k.as_str()))
            .max_by_key(|(k, _)| k.len())
            .map(|(_, v)| v)
    }
}

impl ModelPrice {
    /// Cost of one call in the table's currency.
    pub fn cost(
        &self,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write_5m: i64,
        cache_write_1h: i64,
    ) -> f64 {
        let m = 1e-6;
        input as f64 * self.input * m
            + output as f64 * self.output * m
            + cache_read as f64 * self.cache_read * m
            + cache_write_5m as f64 * self.cache_write_5m * m
            + cache_write_1h as f64 * self.cache_write_1h * m
    }
}
