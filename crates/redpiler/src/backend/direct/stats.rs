//! Full in/out-degree report for the compiled graph (BACKLOG.md BUG-043).
//!
//! The report is Markdown so it can be dropped into any viewer or committed
//! next to the circuit; `src/plot/mod.rs` writes it into `logs/`.

use super::node::NodeType;
use super::DirectBackend;
use std::collections::BTreeMap;
use std::fmt::Write;

fn type_name(ty: &NodeType) -> &'static str {
    match ty {
        NodeType::Repeater { .. } => "Repeater",
        NodeType::Torch => "Torch",
        NodeType::Comparator { .. } => "Comparator",
        NodeType::Lamp => "Lamp",
        NodeType::Button => "Button",
        NodeType::Lever => "Lever",
        NodeType::PressurePlate => "PressurePlate",
        NodeType::Trapdoor => "Trapdoor",
        NodeType::Wire => "Wire",
        NodeType::Constant => "Constant",
        NodeType::NoteBlock { .. } => "NoteBlock",
    }
}

#[derive(Default)]
struct TypeAgg {
    nodes: u32,
    in_sum: u64,
    in_max: u32,
    out_sum: u64,
    out_max: u32,
    out_uniq_sum: u64,
}

fn degree_histogram(values: &[u32]) -> (BTreeMap<u32, usize>, u32) {
    let mut map: BTreeMap<u32, usize> = BTreeMap::new();
    let mut max = 0;
    for value in values {
        *map.entry(*value).or_insert(0) += 1;
        max = max.max(*value);
    }
    (map, max)
}

/// Writes every degree from 0 to the maximum, including the degrees no node
/// has, so the table is a complete distribution instead of only the observed
/// buckets (BACKLOG.md BUG-043: "全量统计").
fn write_distribution(
    out: &mut String,
    title: &str,
    column: &str,
    histogram: &BTreeMap<u32, usize>,
    max: u32,
    total: usize,
) {
    let _ = writeln!(out, "## {}", title);
    let _ = writeln!(out);
    let _ = writeln!(out, "| {} | nodes | ratio |", column);
    let _ = writeln!(out, "| ---: | ---: | ---: |");
    for degree in 0..=max {
        let nodes = histogram.get(&degree).copied().unwrap_or(0);
        let ratio = if total == 0 {
            0.0
        } else {
            100.0 * nodes as f64 / total as f64
        };
        let _ = writeln!(out, "| {} | {} | {:.3}% |", degree, nodes, ratio);
    }
    let _ = writeln!(out);
}

impl DirectBackend {
    /// Every node's in/out-degree as a Markdown report (BACKLOG.md BUG-043).
    ///
    /// Out-degree is the number of forward links leaving a node; links that
    /// point at the same node are counted individually and the deduplicated
    /// count is reported next to it. In-degree is accumulated in one pass over
    /// all links, so the whole report is `O(V + E)`.
    pub fn degree_report(&self, timestamp: &str) -> String {
        let nodes = self.nodes.inner();
        let count = nodes.len();

        let mut in_deg = vec![0u32; count];
        let mut out_deg = vec![0u32; count];
        let mut out_unique = vec![0u32; count];
        let mut links = 0usize;

        for (idx, node) in nodes.iter().enumerate() {
            let forward = self.forward_links.get(&node.fwd_link_range);
            out_deg[idx] = forward.len() as u32;
            links += forward.len();
            let mut seen: Vec<u32> = Vec::new();
            for link in forward {
                let target = link.node().index();
                if target >= count {
                    continue;
                }
                in_deg[target] += 1;
                if !seen.contains(&(target as u32)) {
                    seen.push(target as u32);
                    out_unique[idx] += 1;
                }
            }
        }

        let mut out = String::with_capacity(16384);
        let _ = writeln!(out, "# MCHPRS redpiler graph degree report");
        let _ = writeln!(out);
        let _ = writeln!(out, "- version: `{}`", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(out, "- generated: {}", timestamp);
        let _ = writeln!(out, "- nodes: {}", count);
        let _ = writeln!(
            out,
            "- links: {} (links to the same target counted individually)",
            links
        );
        let _ = writeln!(out);

        let mut per_type: BTreeMap<&'static str, TypeAgg> = BTreeMap::new();
        for (idx, node) in nodes.iter().enumerate() {
            let agg = per_type.entry(type_name(&node.ty)).or_default();
            agg.nodes += 1;
            agg.in_sum += u64::from(in_deg[idx]);
            agg.in_max = agg.in_max.max(in_deg[idx]);
            agg.out_sum += u64::from(out_deg[idx]);
            agg.out_max = agg.out_max.max(out_deg[idx]);
            agg.out_uniq_sum += u64::from(out_unique[idx]);
        }

        let _ = writeln!(out, "## Node types");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "| type | nodes | in sum | in max | out sum | out max | out sum (unique) |"
        );
        let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: | ---: | ---: |");
        for (name, agg) in &per_type {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} |",
                name,
                agg.nodes,
                agg.in_sum,
                agg.in_max,
                agg.out_sum,
                agg.out_max,
                agg.out_uniq_sum
            );
        }
        let _ = writeln!(out);

        let (in_hist, in_max) = degree_histogram(&in_deg);
        let (out_hist, out_max) = degree_histogram(&out_deg);
        write_distribution(
            &mut out,
            "In-degree distribution",
            "in-degree",
            &in_hist,
            in_max,
            count,
        );
        write_distribution(
            &mut out,
            "Out-degree distribution",
            "out-degree",
            &out_hist,
            out_max,
            count,
        );

        let mut hot: Vec<(u32, usize)> = out_deg
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, degree)| *degree >= 2)
            .map(|(idx, degree)| (degree, idx))
            .collect();
        hot.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let _ = writeln!(out, "## Nodes with out-degree >= 2 ({})", hot.len());
        let _ = writeln!(out);
        let _ = writeln!(out, "| pos | type | in | out | out (unique) |");
        let _ = writeln!(out, "| --- | --- | ---: | ---: | ---: |");
        for (degree, idx) in &hot {
            let pos = self.blocks[*idx]
                .first()
                .map(|(pos, _)| pos.to_string())
                .unwrap_or_else(|| "-".to_string());
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |",
                pos,
                type_name(&nodes[*idx].ty),
                in_deg[*idx],
                degree,
                out_unique[*idx]
            );
        }
        let _ = writeln!(out);

        let mut sinks: Vec<(u32, usize)> = in_deg
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, degree)| *degree >= 2)
            .map(|(idx, degree)| (degree, idx))
            .collect();
        sinks.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let _ = writeln!(out, "## Nodes with in-degree >= 2 ({})", sinks.len());
        let _ = writeln!(out);
        let _ = writeln!(out, "| pos | type | in | out |");
        let _ = writeln!(out, "| --- | --- | ---: | ---: |");
        for (degree, idx) in &sinks {
            let pos = self.blocks[*idx]
                .first()
                .map(|(pos, _)| pos.to_string())
                .unwrap_or_else(|| "-".to_string());
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                pos,
                type_name(&nodes[*idx].ty),
                degree,
                out_deg[*idx]
            );
        }
        let _ = writeln!(out);

        let _ = writeln!(out, "## Notes");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "- Counts the backend's forward links: who gets notified when a node changes."
        );
        let _ = writeln!(
            out,
            "- In-degree counts every link; several links from the same source count more than once."
        );
        let _ = writeln!(
            out,
            "- Wire networks are sinks in this graph (out-degree 0) by design; read their in-degree instead."
        );
        let _ = writeln!(
            out,
            "- Compile options are those of this run (`/redpiler stats` uses the default options, i.e. no `--optimize`)."
        );

        out
    }
}
