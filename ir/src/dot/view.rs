use std::fmt::{Display, Formatter, Result};

use crate::data_flow::node::Simple;

#[derive(Clone, Copy)]
pub enum NodeType {
	Gamma,
	Theta,
	Lambda,
	Phi,
	Then,
	Reachable,
	NotReachable,
}

#[derive(Clone, Copy)]
pub struct Color(pub NodeType);

impl Display for Color {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		let color = match self.0 {
			NodeType::Gamma => "8b81e8",
			NodeType::Theta => "bb84ca",
			NodeType::Lambda => "dde881",
			NodeType::Phi => "e6b79a",
			NodeType::Then => "89b7d7",
			NodeType::Reachable => "81e8bf",
			NodeType::NotReachable => "e881aa",
		};

		write!(f, "\"#{color}\"")
	}
}

#[derive(Clone, Copy)]
pub struct Label(pub NodeType);

impl Display for Label {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		let name = match self.0 {
			NodeType::Gamma => "Gamma",
			NodeType::Theta => "Theta",
			NodeType::Lambda => "Lambda",
			NodeType::Phi => "Phi",
			NodeType::Then => "Then",
			NodeType::Reachable => "Reachable",
			NodeType::NotReachable => "Not Reachable",
		};

		write!(f, r#""{name}""#)
	}
}

#[derive(Clone, Copy)]
pub struct Ports(pub &'static str, pub usize);

impl Display for Ports {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		if self.1 == 0 {
			return Ok(());
		}

		let post = self.0;

		write!(f, "<TR>")?;

		(0..self.1).try_for_each(|i| write!(f, r#"<TD PORT="{post}{i}">{i}</TD>"#))?;

		write!(f, "</TR>")
	}
}

#[derive(Clone, Copy)]
pub struct Mnemonic<'a>(pub &'a Simple);

impl Display for Mnemonic<'_> {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self.0 {
			Simple::RegionStart(_) => "Start".fmt(f),
			Simple::RegionEnd(_) => "End".fmt(f),
		}
	}
}
