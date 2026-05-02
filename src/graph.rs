//! `sw-launch graph <scenario>`: render the layer DAG.
//!
//! Phase 4 step 5. Lets users (and agents) see what a scenario
//! depends on without running it. Pure presentation logic over
//! `Config` -- no tools spawned, no artifacts built.
//!
//! Edges come from cross-layer patches: any patch with
//! `target = <layer>.<symbol>` (or `value = <layer>.<symbol>`)
//! introduces an edge from the patching layer to the patched
//! layer. Layers without explicit edges retain the order they
//! were declared in `scenario.layers`, then chained one after
//! the next so the text rendering still shows a tree.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8Path;

use crate::config::Config;
use crate::error::{Error, Result};

/// One node in the rendered DAG.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub name: String,
    pub kind: String,
    pub input: Option<String>,
    pub depends_on: Vec<String>,
}

/// The full graph for one scenario.
#[derive(Debug, Clone)]
pub struct Graph {
    pub scenario: String,
    pub target: String,
    pub layers: Vec<GraphNode>,
}

/// Subcommand entry point.
pub fn run(config_path: &Utf8Path, scenario: &str, json: bool) -> Result<()> {
    let cfg = Config::from_path(config_path)?;
    let graph = build(&cfg, scenario)?;
    if json {
        println!("{}", graph.to_json());
    } else {
        graph.print_text();
    }
    Ok(())
}

/// Build a [`Graph`] from a parsed `Config` + scenario name.
pub fn build(cfg: &Config, scenario: &str) -> Result<Graph> {
    let scen = cfg
        .scenarios
        .get(scenario)
        .ok_or_else(|| Error::cli(format!("scenario `{scenario}` not declared")))?;
    let mut nodes: Vec<GraphNode> = Vec::new();
    let layer_set: BTreeSet<&str> = scen.layers.iter().map(String::as_str).collect();
    for (idx, name) in scen.layers.iter().enumerate() {
        let layer = cfg.layers.get(name).ok_or_else(|| {
            Error::cli(format!(
                "scenario `{scenario}` references undeclared layer `{name}`"
            ))
        })?;
        let mut deps: BTreeSet<String> = BTreeSet::new();
        // Implicit dep on the prior layer in declaration order so
        // a chain still renders even with no patches.
        if idx > 0 {
            deps.insert(scen.layers[idx - 1].clone());
        }
        for patch in &layer.patches {
            for term in [&patch.target, &patch.value] {
                if let Some(other) = referenced_layer(term)
                    && layer_set.contains(other)
                    && other != name.as_str()
                {
                    deps.insert(other.to_string());
                }
            }
        }
        nodes.push(GraphNode {
            name: name.clone(),
            kind: layer.kind.clone(),
            input: layer.input.clone(),
            depends_on: deps.into_iter().collect(),
        });
    }
    Ok(Graph {
        scenario: scenario.to_string(),
        target: scen.target.clone(),
        layers: nodes,
    })
}

fn referenced_layer(term: &str) -> Option<&str> {
    if term.starts_with("0x") || term.starts_with("sidecar:") {
        return None;
    }
    if term == "self.address" || term == "self.end" || term == "self.size" {
        return None;
    }
    let (layer, _) = term.split_once('.')?;
    if layer == "self" { None } else { Some(layer) }
}

impl Graph {
    fn print_text(&self) {
        println!("{} (target: {})", self.scenario, self.target);
        let mut depth_of: BTreeMap<&str, usize> = BTreeMap::new();
        for node in &self.layers {
            let depth = node
                .depends_on
                .iter()
                .filter_map(|d| depth_of.get(d.as_str()).copied())
                .max()
                .map(|d| d + 1)
                .unwrap_or(0);
            depth_of.insert(node.name.as_str(), depth);
            let indent = "  ".repeat(depth);
            let arrow = if depth > 0 { "-> " } else { "" };
            let input = node.input.as_deref().unwrap_or("-");
            println!(
                "{indent}{arrow}{name}  ({kind}: {input})",
                name = node.name,
                kind = node.kind,
            );
        }
    }

    /// Render as a JSON object. Hand-rolled to avoid pulling in
    /// serde_json for a single-shape output.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"scenario\":\"");
        out.push_str(&escape(&self.scenario));
        out.push_str("\",\"target\":\"");
        out.push_str(&escape(&self.target));
        out.push_str("\",\"layers\":[");
        for (i, node) in self.layers.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":\"");
            out.push_str(&escape(&node.name));
            out.push_str("\",\"kind\":\"");
            out.push_str(&escape(&node.kind));
            out.push_str("\",\"input\":");
            match &node.input {
                Some(s) => {
                    out.push('"');
                    out.push_str(&escape(s));
                    out.push('"');
                }
                None => out.push_str("null"),
            }
            out.push_str(",\"depends_on\":[");
            for (j, d) in node.depends_on.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(&escape(d));
                out.push('"');
            }
            out.push_str("]}");
        }
        out.push_str("]}");
        out
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
