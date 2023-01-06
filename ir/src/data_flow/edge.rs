use std::ops::Range;

use super::node::NodeId;

// We use custom end points instead of `Range` because we
// want `Copy` and inclusive behavior.
#[derive(Default, Clone, Copy)]
pub struct Edge {
	node: NodeId,
	start: usize,
	end: usize,
}

impl Edge {
	#[must_use]
	pub fn at_range(node: NodeId, start: usize, end: usize) -> Self {
		Self { node, start, end }
	}

	#[must_use]
	pub fn at_port(node: NodeId, port: usize) -> Self {
		Self::at_range(node, port, port)
	}

	#[must_use]
	pub fn at(node: NodeId) -> Self {
		Self::at_port(node, 0)
	}

	#[must_use]
	pub fn node(self) -> NodeId {
		self.node
	}

	#[must_use]
	#[allow(clippy::range_plus_one)]
	pub fn ports(self) -> Range<usize> {
		self.start..self.end + 1
	}
}
