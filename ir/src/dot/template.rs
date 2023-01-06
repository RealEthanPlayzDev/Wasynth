use std::{
	fmt::Display,
	io::{Result, Write},
};

use crate::data_flow::{graph::Graph, node::NodeId};

use super::view::{Color, Label, NodeType, Ports};

#[derive(Clone, Copy)]
pub struct Summary {
	incoming: usize,
	outgoing: usize,
	id: NodeId,
}

impl Summary {
	pub fn new(id: NodeId, graph: &Graph) -> Self {
		let incoming = graph.incoming[id].iter().flat_map(|v| v.ports()).count();
		let outgoing = 0;

		Self {
			incoming,
			outgoing,
			id,
		}
	}

	pub fn set_outgoing(&mut self, outgoing: usize) {
		self.outgoing = self.outgoing.max(outgoing);
	}

	pub fn write(&self, w: &mut dyn Write, label: &dyn Display) -> Result<()> {
		let incoming = Ports("i", self.incoming);
		let outgoing = Ports("o", self.outgoing);
		let id = self.id;
		let color = "#DDDDFF";

		write!(w, "{id} [group = Simple, label = <")?;
		write!(w, r#"<TABLE BGCOLOR="{color}" BORDER="1" CELLSPACING="0">"#)?;
		write!(w, "{incoming}<TR><TD>{label}</TD></TR>{outgoing}")?;
		write!(w, "</TABLE>")?;
		writeln!(w, ">];")
	}
}

pub struct Cluster<T> {
	name: T,
	typ: NodeType,
}

impl<T> Cluster<T>
where
	T: Display,
{
	pub fn new(name: T, typ: NodeType) -> Self {
		Self { name, typ }
	}

	pub fn write<M>(&self, w: &mut dyn Write, label: &dyn Display, mut nested: M) -> Result<()>
	where
		M: FnMut(&mut dyn Write) -> Result<()>,
	{
		let name = &self.name;
		let color = Color(self.typ);

		writeln!(w, "subgraph cluster_{name} {{")?;
		writeln!(w, "fillcolor = {color};")?;
		writeln!(w, "label = {label};")?;

		nested(w)?;

		writeln!(w, "}}")
	}

	pub fn write_labeled<M>(&self, w: &mut dyn Write, nested: M) -> Result<()>
	where
		M: FnMut(&mut dyn Write) -> Result<()>,
	{
		let label = Label(self.typ);

		self.write(w, &label, nested)
	}
}
