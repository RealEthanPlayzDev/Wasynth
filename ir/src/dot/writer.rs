use std::{
	collections::HashSet,
	io::{Result, Write},
};

use slotmap::SecondaryMap;

use crate::data_flow::{
	edge::Edge,
	graph::Graph,
	node::{Compound, Node, NodeId, Region, Simple},
};

use super::{
	template::{Cluster, Summary},
	view::{Mnemonic, NodeType},
};

fn get_node_head_id(id: NodeId, graph: &Graph) -> NodeId {
	let Some(compound) = graph.nodes[id].as_compound() else { return id };

	match compound {
		Compound::Gamma(_) => id,
		Compound::Theta(theta) => theta.region().end(),
		Compound::Lambda(lambda) => lambda.region().end(),
		Compound::Phi(phi) => phi.region().end(),
	}
}

fn add_edge(w: &mut dyn Write, from: Edge, to: Edge, graph: &Graph) -> Result<()> {
	let from_id = get_node_head_id(from.node(), graph);
	let to_id = to.node();

	from.ports()
		.zip(to.ports())
		.try_for_each(|(o, i)| writeln!(w, "{from_id}:o{o} -> {to_id}:i{i};"))
}

fn add_edges_redirected(w: &mut dyn Write, to: NodeId, from: NodeId, graph: &Graph) -> Result<()> {
	let mut start = 0;

	graph.incoming[from].iter().copied().try_for_each(|from| {
		let end = start + from.ports().len();
		let to = Edge::at_range(to, start, end - 1);

		start = end;

		add_edge(w, from, to, graph)
	})
}

fn add_edges_incoming(w: &mut dyn Write, to: NodeId, graph: &Graph) -> Result<()> {
	add_edges_redirected(w, to, to, graph)
}

#[derive(Default)]
pub struct Writer {
	nodes_visited: HashSet<NodeId>,
	node_info: SecondaryMap<NodeId, Summary>,
}

impl Writer {
	/// # Errors
	///
	/// If writing to the writer fails.
	pub fn write(&mut self, w: &mut dyn Write, graph: &Graph) -> Result<()> {
		writeln!(w, "digraph {{")?;
		writeln!(w, "node [shape = none];")?;
		writeln!(w, "style = filled;")?;

		self.nodes_visited.clear();
		self.initialize_nodes(graph);
		self.add_reachable(w, graph)?;
		self.add_not_unreachable(w, graph)?;

		writeln!(w, "}}")
	}

	fn add_nodes_incoming(&mut self, w: &mut dyn Write, id: NodeId, graph: &Graph) -> Result<()> {
		graph.incoming[id]
			.iter()
			.map(|e| e.node())
			.try_for_each(|n| self.maybe_add_node(w, n, graph))
	}

	fn add_simple(
		&mut self,
		w: &mut dyn Write,
		simple: &Simple,
		id: NodeId,
		graph: &Graph,
	) -> Result<()> {
		self.add_nodes_incoming(w, id, graph)?;

		let mnemonic = Mnemonic(simple);

		self.node_info[id].write(w, &mnemonic)?;

		add_edges_incoming(w, id, graph)
	}

	fn add_gamma(
		&mut self,
		w: &mut dyn Write,
		regions: &[Region],
		id: NodeId,
		graph: &Graph,
	) -> Result<()> {
		Cluster::new(id, NodeType::Gamma).write_labeled(w, |w| {
			Summary::new(id, graph).write(w, &"Selector")?;
			add_edges_incoming(w, id, graph)?;

			regions.iter().enumerate().try_for_each(|(i, v)| {
				Cluster::new(v.start(), NodeType::Then).write(w, &i, |w| {
					self.maybe_add_node(w, v.start(), graph)?;
					self.maybe_add_node(w, v.end(), graph)
				})
			})
		})
	}

	fn add_generic(
		&mut self,
		w: &mut dyn Write,
		region: Region,
		typ: NodeType,
		id: NodeId,
		graph: &Graph,
	) -> Result<()> {
		add_edges_redirected(w, region.start(), id, graph)?;

		Cluster::new(region.start(), typ).write_labeled(w, |w| {
			self.maybe_add_node(w, region.start(), graph)?;
			self.maybe_add_node(w, region.end(), graph)
		})
	}

	fn add_compound(
		&mut self,
		w: &mut dyn Write,
		compound: &Compound,
		id: NodeId,
		graph: &Graph,
	) -> Result<()> {
		match compound {
			Compound::Gamma(v) => self.add_gamma(w, v.regions(), id, graph),
			Compound::Theta(v) => self.add_generic(w, v.region(), NodeType::Theta, id, graph),
			Compound::Lambda(v) => self.add_generic(w, v.region(), NodeType::Lambda, id, graph),
			Compound::Phi(v) => self.add_generic(w, v.region(), NodeType::Phi, id, graph),
		}
	}

	fn add_node(&mut self, w: &mut dyn Write, id: NodeId, graph: &Graph) -> Result<()> {
		let Some(node) = graph.nodes.get(id) else { return Summary::new(id, graph).write(w, &"???") };

		match node {
			Node::Simple(simple) => self.add_simple(w, simple, id, graph),
			Node::Compound(compound) => self.add_compound(w, compound, id, graph),
		}
	}

	fn maybe_add_node(&mut self, w: &mut dyn Write, id: NodeId, graph: &Graph) -> Result<()> {
		if self.nodes_visited.insert(id) {
			self.add_node(w, id, graph)
		} else {
			Ok(())
		}
	}

	fn initialize_nodes(&mut self, graph: &Graph) {
		self.node_info.clear();

		for id in graph.nodes.keys() {
			let info = Summary::new(id, graph);

			self.node_info.insert(id, info);
		}

		for edge in graph.incoming.values().flatten() {
			let id = get_node_head_id(edge.node(), graph);
			let end = edge.ports().end;

			self.node_info[id].set_outgoing(end);
		}
	}

	fn add_reachable(&mut self, w: &mut dyn Write, graph: &Graph) -> Result<()> {
		let Some(id) = graph.start else { return Ok(()) };

		Cluster::new("reachable", NodeType::Reachable)
			.write_labeled(w, |w| self.maybe_add_node(w, id, graph))
	}

	fn add_not_unreachable(&mut self, w: &mut dyn Write, graph: &Graph) -> Result<()> {
		Cluster::new("unreachable", NodeType::NotReachable).write_labeled(w, |w| {
			graph
				.nodes
				.keys()
				.try_for_each(|v| self.maybe_add_node(w, v, graph))
		})
	}
}
