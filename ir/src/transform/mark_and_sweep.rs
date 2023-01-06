use std::collections::HashSet;

use crate::data_flow::{graph::Graph, node::NodeId};

#[derive(Default)]
pub struct MarkAndSweep {
	nodes_visited: HashSet<NodeId>,
	nodes_pending: Vec<NodeId>,
}

impl MarkAndSweep {
	fn mark_node_at(&mut self, id: NodeId) {
		if self.nodes_visited.contains(&id) {
			return;
		}

		self.nodes_visited.insert(id);
		self.nodes_pending.push(id);
	}

	fn mark_edges_at(&mut self, graph: &Graph, id: NodeId) {
		if let Some(regions) = graph.nodes[id].as_regions() {
			for region in regions {
				self.mark_node_at(region.start());
				self.mark_node_at(region.end());
			}
		}

		for edge in &graph.incoming[id] {
			self.mark_node_at(edge.node());
		}
	}

	fn mark(&mut self, graph: &Graph) {
		if let Some(start) = graph.start {
			self.mark_node_at(start);
		}

		while let Some(id) = self.nodes_pending.pop() {
			self.mark_edges_at(graph, id);
		}
	}

	fn sweep(&mut self, graph: &mut Graph) {
		graph.nodes.retain(|i, _| self.nodes_visited.contains(&i));
		graph
			.incoming
			.retain(|i, _| self.nodes_visited.contains(&i));
	}

	pub fn run(&mut self, graph: &mut Graph) {
		self.nodes_visited.clear();

		self.mark(graph);
		self.sweep(graph);
	}
}
