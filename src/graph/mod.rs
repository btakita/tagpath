use crate::extract;
use crate::parser;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use std::collections::HashMap;
use std::path::Path;

/// A directed graph of tag co-occurrence relationships.
///
/// Nodes are individual tags (lowercase, deduplicated).
/// Edges connect sequential tag pairs within identifiers,
/// with weight = number of identifiers containing that pair.
pub struct TagGraph {
	pub graph: DiGraph<String, u32>,
	pub node_map: HashMap<String, NodeIndex>,
}

/// Build a tag co-occurrence graph from all identifiers found under `path`.
///
/// For each extracted identifier with tags `[a, b, c]`, creates edges
/// a->b and b->c. Edge weights count how many identifiers share that pair.
pub fn build_graph(path: &Path) -> TagGraph {
	let identifiers = extract::extract_from_path(path);
	let mut graph = DiGraph::<String, u32>::new();
	let mut node_map: HashMap<String, NodeIndex> = HashMap::new();
	for ident in &identifiers {
		let tags = &ident.parsed.tags;
		if tags.len() < 2 {
			// Single-tag identifiers produce no edges
			// but still register as nodes
			for tag in tags {
				node_map
					.entry(tag.clone())
					.or_insert_with(|| graph.add_node(tag.clone()));
			}
			continue;
		}
		// Ensure all tags are nodes
		for tag in tags {
			node_map
				.entry(tag.clone())
				.or_insert_with(|| graph.add_node(tag.clone()));
		}
		// Add edges for sequential tag pairs
		for pair in tags.windows(2) {
			let from = node_map[&pair[0]];
			let to = node_map[&pair[1]];
			// Find existing edge and increment, or add new
			if let Some(edge) = graph.find_edge(from, to) {
				let w = graph.edge_weight_mut(edge).unwrap();
				*w += 1;
			} else {
				graph.add_edge(from, to, 1);
			}
		}
	}
	TagGraph { graph, node_map }
}

/// Filter the graph to a subgraph reachable from query tags within 1 hop.
///
/// Returns a new `TagGraph` containing only the matched nodes, their direct
/// neighbors, and the connecting edges.
pub fn filter_by_query(
	tag_graph: &TagGraph,
	query: &str,
) -> TagGraph {
	let parsed = parser::parse(
		query,
		parser::detect_convention(query),
	);
	let query_tags: Vec<&str> =
		parsed.tags.iter().map(|s| s.as_str()).collect();
	// Collect seed nodes matching query tags
	let mut included_nodes: Vec<NodeIndex> = Vec::new();
	for qt in &query_tags {
		if let Some(&idx) = tag_graph.node_map.get(*qt) {
			included_nodes.push(idx);
		}
	}
	// Expand 1 hop: add direct neighbors (both directions)
	let mut all_nodes: Vec<NodeIndex> = included_nodes.clone();
	for &node in &included_nodes {
		for edge in tag_graph.graph.edges(node) {
			all_nodes.push(edge.target());
		}
		// Also include incoming edges
		for edge in tag_graph
			.graph
			.edges_directed(node, petgraph::Direction::Incoming)
		{
			all_nodes.push(edge.source());
		}
	}
	all_nodes.sort();
	all_nodes.dedup();
	// Build new graph with only included nodes and their connecting edges
	let mut new_graph = DiGraph::<String, u32>::new();
	let mut new_node_map: HashMap<String, NodeIndex> =
		HashMap::new();
	let mut old_to_new: HashMap<NodeIndex, NodeIndex> =
		HashMap::new();
	for &old_idx in &all_nodes {
		let label = &tag_graph.graph[old_idx];
		let new_idx = new_graph.add_node(label.clone());
		new_node_map.insert(label.clone(), new_idx);
		old_to_new.insert(old_idx, new_idx);
	}
	// Add edges between included nodes
	for edge in tag_graph.graph.edge_indices() {
		let (src, tgt) = tag_graph
			.graph
			.edge_endpoints(edge)
			.unwrap();
		if let (Some(&new_src), Some(&new_tgt)) =
			(old_to_new.get(&src), old_to_new.get(&tgt))
		{
			let weight = *tag_graph.graph.edge_weight(edge).unwrap();
			new_graph.add_edge(new_src, new_tgt, weight);
		}
	}
	TagGraph {
		graph: new_graph,
		node_map: new_node_map,
	}
}

/// Render the graph (or a query-filtered subgraph) as DOT format.
pub fn to_dot(
	tag_graph: &TagGraph,
	query: Option<&str>,
) -> String {
	let effective = match query {
		Some(q) => filter_by_query(tag_graph, q),
		None => {
			// Use the original graph reference directly
			return render_dot(tag_graph);
		}
	};
	render_dot(&effective)
}

fn render_dot(tag_graph: &TagGraph) -> String {
	let mut out = String::from("digraph tags {\n\trankdir=LR;\n");
	// Collect and sort edges for deterministic output
	let mut edges: Vec<(String, String, u32)> = Vec::new();
	for edge in tag_graph.graph.edge_indices() {
		let (src, tgt) = tag_graph
			.graph
			.edge_endpoints(edge)
			.unwrap();
		let weight = *tag_graph.graph.edge_weight(edge).unwrap();
		edges.push((
			tag_graph.graph[src].clone(),
			tag_graph.graph[tgt].clone(),
			weight,
		));
	}
	edges.sort();
	for (from, to, weight) in &edges {
		out.push_str(&format!(
			"\t\"{}\" -> \"{}\" [label=\"{}\"];\n",
			from, to, weight,
		));
	}
	out.push_str("}\n");
	out
}

/// Render the graph (or a query-filtered subgraph) as JSON.
pub fn to_json(
	tag_graph: &TagGraph,
	query: Option<&str>,
) -> serde_json::Value {
	let effective = match query {
		Some(q) => filter_by_query(tag_graph, q),
		None => {
			return render_json(tag_graph);
		}
	};
	render_json(&effective)
}

fn render_json(tag_graph: &TagGraph) -> serde_json::Value {
	let mut nodes: Vec<String> = tag_graph
		.node_map
		.keys()
		.cloned()
		.collect();
	nodes.sort();
	let mut edges: Vec<serde_json::Value> = Vec::new();
	for edge in tag_graph.graph.edge_indices() {
		let (src, tgt) = tag_graph
			.graph
			.edge_endpoints(edge)
			.unwrap();
		let weight = *tag_graph.graph.edge_weight(edge).unwrap();
		edges.push(serde_json::json!({
			"from": tag_graph.graph[src],
			"to": tag_graph.graph[tgt],
			"weight": weight,
		}));
	}
	// Sort edges for deterministic output
	edges.sort_by(|a, b| {
		let a_from = a["from"].as_str().unwrap_or("");
		let b_from = b["from"].as_str().unwrap_or("");
		let a_to = a["to"].as_str().unwrap_or("");
		let b_to = b["to"].as_str().unwrap_or("");
		(a_from, a_to).cmp(&(b_from, b_to))
	});
	serde_json::json!({
		"nodes": nodes,
		"edges": edges,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	/// Create a temp directory with known source files for testing.
	fn setup_test_dir(name: &str) -> std::path::PathBuf {
		let dir =
			std::env::temp_dir().join(format!("tagpath_test_graph_{name}"));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	fn write_file(dir: &Path, filename: &str, content: &str) {
		let path = dir.join(filename);
		let mut f = std::fs::File::create(&path).unwrap();
		write!(f, "{}", content).unwrap();
	}

	#[test]
	fn test_build_graph_nodes_and_edges() {
		let dir = setup_test_dir("build");
		write_file(
			&dir,
			"sample.rs",
			"fn create_user() {}\nfn validate_user() {}\nfn create_profile() {}\n",
		);
		let tg = build_graph(&dir);
		// Should have nodes: create, user, validate, profile
		assert!(tg.node_map.contains_key("create"));
		assert!(tg.node_map.contains_key("user"));
		assert!(tg.node_map.contains_key("validate"));
		assert!(tg.node_map.contains_key("profile"));
		// create->user should have weight 1 (from create_user)
		let create_idx = tg.node_map["create"];
		let user_idx = tg.node_map["user"];
		let edge = tg.graph.find_edge(create_idx, user_idx).unwrap();
		assert_eq!(*tg.graph.edge_weight(edge).unwrap(), 1);
		// validate->user should exist
		let validate_idx = tg.node_map["validate"];
		assert!(tg.graph.find_edge(validate_idx, user_idx).is_some());
		// create->profile should exist
		let profile_idx = tg.node_map["profile"];
		assert!(tg.graph.find_edge(create_idx, profile_idx).is_some());
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_edge_weight_accumulates() {
		let dir = setup_test_dir("weight");
		// Two identifiers with same tag pair: create_user appears twice
		write_file(
			&dir,
			"a.rs",
			"fn create_user() {}\n",
		);
		write_file(
			&dir,
			"b.rs",
			"fn create_user_profile() {}\n",
		);
		let tg = build_graph(&dir);
		let create_idx = tg.node_map["create"];
		let user_idx = tg.node_map["user"];
		let edge = tg.graph.find_edge(create_idx, user_idx).unwrap();
		// create->user appears in both create_user and create_user_profile
		assert_eq!(*tg.graph.edge_weight(edge).unwrap(), 2);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_query_filter() {
		let dir = setup_test_dir("query");
		write_file(
			&dir,
			"sample.rs",
			"fn create_user() {}\nfn validate_email() {}\nfn get_profile() {}\n",
		);
		let tg = build_graph(&dir);
		// Query for "user" — should include user + direct neighbors (create)
		let filtered = filter_by_query(&tg, "user");
		assert!(filtered.node_map.contains_key("user"));
		assert!(filtered.node_map.contains_key("create"));
		// "email" and "validate" should NOT be in the user subgraph
		assert!(!filtered.node_map.contains_key("email"));
		assert!(!filtered.node_map.contains_key("validate"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_dot_output() {
		let dir = setup_test_dir("dot");
		write_file(
			&dir,
			"sample.rs",
			"fn create_user() {}\n",
		);
		let tg = build_graph(&dir);
		let dot = to_dot(&tg, None);
		assert!(dot.starts_with("digraph tags {"));
		assert!(dot.contains("\"create\" -> \"user\""));
		assert!(dot.contains("[label=\"1\"]"));
		assert!(dot.ends_with("}\n"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_json_output() {
		let dir = setup_test_dir("json");
		write_file(
			&dir,
			"sample.rs",
			"fn create_user() {}\n",
		);
		let tg = build_graph(&dir);
		let json = to_json(&tg, None);
		let nodes = json["nodes"].as_array().unwrap();
		let node_strs: Vec<&str> =
			nodes.iter().map(|n| n.as_str().unwrap()).collect();
		assert!(node_strs.contains(&"create"));
		assert!(node_strs.contains(&"user"));
		let edges = json["edges"].as_array().unwrap();
		assert!(!edges.is_empty());
		let first_edge = &edges[0];
		assert_eq!(first_edge["from"], "create");
		assert_eq!(first_edge["to"], "user");
		assert_eq!(first_edge["weight"], 1);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_dot_with_query() {
		let dir = setup_test_dir("dot_query");
		write_file(
			&dir,
			"sample.rs",
			"fn create_user() {}\nfn validate_email() {}\n",
		);
		let tg = build_graph(&dir);
		let dot = to_dot(&tg, Some("user"));
		assert!(dot.contains("\"create\" -> \"user\""));
		// validate->email should NOT appear in user-centric subgraph
		assert!(!dot.contains("\"validate\" -> \"email\""));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_json_with_query() {
		let dir = setup_test_dir("json_query");
		write_file(
			&dir,
			"sample.rs",
			"fn create_user() {}\nfn validate_email() {}\n",
		);
		let tg = build_graph(&dir);
		let json = to_json(&tg, Some("user"));
		let nodes = json["nodes"].as_array().unwrap();
		let node_strs: Vec<&str> =
			nodes.iter().map(|n| n.as_str().unwrap()).collect();
		assert!(node_strs.contains(&"user"));
		assert!(node_strs.contains(&"create"));
		assert!(!node_strs.contains(&"email"));
		assert!(!node_strs.contains(&"validate"));
		let _ = std::fs::remove_dir_all(&dir);
	}
}
