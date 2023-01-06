use std::fmt::{Display, Formatter, Result};

use slotmap::{new_key_type, Key};

new_key_type! { pub struct NodeId; }

impl Display for NodeId {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		let data = self.data();

		write!(f, "N{data:?}")
	}
}

macro_rules! impl_from_data {
	($type:tt, $name:tt) => {
		impl From<$name> for Node {
			fn from(data: $name) -> Self {
				Self::$type($type::$name(data))
			}
		}
	};
}

macro_rules! impl_from_region {
	($name:tt) => {
		impl From<Region> for $name {
			fn from(region: Region) -> Self {
				Self { region }
			}
		}

		impl $name {
			#[must_use]
			pub fn region(&self) -> Region {
				self.region
			}
		}
	};
}

macro_rules! impl_simple {
	($name:tt) => {
		impl_from_data!(Simple, $name);
	};
}

macro_rules! impl_compound {
	($name:tt) => {
		impl_from_data!(Compound, $name);
	};
}

pub struct RegionStart;

impl_simple!(RegionStart);

pub struct RegionEnd;

impl_simple!(RegionEnd);

pub enum Simple {
	RegionStart(RegionStart),
	RegionEnd(RegionEnd),
}

#[derive(Clone, Copy)]
pub struct Region {
	start: NodeId,
	end: NodeId,
}

impl Region {
	pub(crate) fn new(start: NodeId, end: NodeId) -> Self {
		Self { start, end }
	}

	#[must_use]
	pub fn start(self) -> NodeId {
		self.start
	}

	#[must_use]
	pub fn end(self) -> NodeId {
		self.end
	}
}

pub struct Gamma {
	regions: Box<[Region]>,
}

impl Gamma {
	#[must_use]
	pub fn regions(&self) -> &[Region] {
		&self.regions
	}
}

impl_compound!(Gamma);

impl From<Box<[Region]>> for Gamma {
	fn from(regions: Box<[Region]>) -> Self {
		Self { regions }
	}
}

pub struct Theta {
	region: Region,
}

impl_compound!(Theta);
impl_from_region!(Theta);

pub struct Lambda {
	region: Region,
}

impl_compound!(Lambda);
impl_from_region!(Lambda);

pub struct Phi {
	region: Region,
}

impl_compound!(Phi);
impl_from_region!(Phi);

pub enum Compound {
	Gamma(Gamma),
	Theta(Theta),
	Lambda(Lambda),
	Phi(Phi),
}

impl Compound {
	#[must_use]
	pub fn as_regions(&self) -> &[Region] {
		match self {
			Self::Gamma(gamma) => &gamma.regions,
			Self::Theta(theta) => std::slice::from_ref(&theta.region),
			Self::Lambda(lambda) => std::slice::from_ref(&lambda.region),
			Self::Phi(phi) => std::slice::from_ref(&phi.region),
		}
	}
}

pub enum Node {
	Simple(Simple),
	Compound(Compound),
}

impl Node {
	#[must_use]
	pub fn as_simple(&self) -> Option<&Simple> {
		match &self {
			Self::Simple(simple) => Some(simple),
			Self::Compound(..) => None,
		}
	}

	#[must_use]
	pub fn as_compound(&self) -> Option<&Compound> {
		match &self {
			Self::Simple(..) => None,
			Self::Compound(compound) => Some(compound),
		}
	}

	#[must_use]
	pub fn as_regions(&self) -> Option<&[Region]> {
		self.as_compound().map(Compound::as_regions)
	}
}
