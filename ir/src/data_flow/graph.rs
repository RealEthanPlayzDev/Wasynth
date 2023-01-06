use slotmap::{SecondaryMap, SlotMap};
use tinyvec::TinyVec;

use super::{
	edge::Edge,
	node::{Gamma, Node, NodeId, Region, RegionEnd, RegionStart},
};

type EdgeList = TinyVec<[Edge; 2]>;

#[derive(Default)]
pub struct Graph {
	pub start: Option<NodeId>,
	pub nodes: SlotMap<NodeId, Node>,
	pub incoming: SecondaryMap<NodeId, EdgeList>,
}

impl Graph {
	pub fn clear(&mut self) {
		self.start = None;
		self.nodes.clear();
		self.incoming.clear();
	}

	#[must_use]
	pub fn add_node<T>(&mut self, node: T) -> NodeId
	where
		T: Into<Node>,
	{
		let id = self.nodes.insert(node.into());

		self.incoming.insert(id, EdgeList::new());

		id
	}

	#[must_use]
	pub fn add_region(&mut self) -> Region {
		let start = self.add_node(RegionStart);
		let end = self.add_node(RegionEnd);

		Region::new(start, end)
	}

	#[must_use]
	pub fn add_compound<T>(&mut self) -> (Region, NodeId)
	where
		T: From<Region> + Into<Node>,
	{
		let region = self.add_region();
		let compound = self.add_node(T::from(region));

		(region, compound)
	}

	#[must_use]
	pub fn add_gamma(&mut self, regions: Box<[Region]>) -> NodeId {
		let gamma = Gamma::from(regions);

		self.add_node(gamma)
	}

	pub fn add_connection(&mut self, from: Edge, to: Edge) {
		debug_assert_eq!(from.ports().len(), to.ports().len());

		self.incoming[to.node()].push(from);
	}
}
