use wasmparser::Operator;

/// A very basic implementation of dead code elimination for WebAssembly that
/// simply removes blocks that have no branches to them.
#[derive(Default)]
pub struct DeadCodeElimination {
	nested_unreachable: usize,
}

impl DeadCodeElimination {
	#[cold]
	fn drop_unreachable(&mut self, op: &Operator) {
		match op {
			Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => {
				self.nested_unreachable += 1;
			}
			Operator::Else if self.nested_unreachable == 1 => {
				self.nested_unreachable -= 1;
			}
			Operator::End => {
				self.nested_unreachable -= 1;
			}
			_ => {}
		}
	}

	fn maybe_end_of_block(&mut self, op: &Operator) {
		if matches!(
			op,
			Operator::Unreachable
				| Operator::Br { .. }
				| Operator::BrTable { .. }
				| Operator::Return
		) {
			self.nested_unreachable += 1;
		}
	}

	fn is_reachable(&self) -> bool {
		self.nested_unreachable == 0
	}

	fn run_tracking<'a>(&mut self, list: Vec<Operator<'a>>) -> Vec<Operator<'a>> {
		let mut remaining = Vec::with_capacity(list.len());

		for op in list {
			let reachable = if self.is_reachable() {
				self.maybe_end_of_block(&op);
				true
			} else {
				self.drop_unreachable(&op);
				self.is_reachable()
			};

			if reachable {
				remaining.push(op);
			}
		}

		remaining
	}

	pub fn run(list: Vec<Operator>) -> Vec<Operator> {
		let mut handler = Self::default();

		handler.run_tracking(list)
	}
}
