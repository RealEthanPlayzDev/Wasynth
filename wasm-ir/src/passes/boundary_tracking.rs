use std::collections::HashMap;

use wasmparser::Operator;

#[derive(PartialEq, Eq, Hash)]
pub enum BoundaryType {
	Loop { start: usize },
	Else,
}

#[derive(Default)]
pub struct BoundaryTracking {
	pending_stack: Vec<Option<BoundaryType>>,

	result_map: HashMap<usize, BoundaryType>,
}

impl BoundaryTracking {
	fn run_tracking(&mut self, code: &[Operator]) {
		for (i, inst) in code.iter().enumerate() {
			match inst {
				Operator::Block { .. } | Operator::If { .. } => {
					self.pending_stack.push(None);
				}
				Operator::Loop { .. } => {
					let data = BoundaryType::Loop { start: i };

					self.pending_stack.push(Some(data));
				}
				Operator::Else => {
					let data = BoundaryType::Else;

					*self.pending_stack.last_mut().unwrap() = Some(data);
				}
				Operator::End { .. } => {
					let boundary = self.pending_stack.pop().unwrap();

					if let Some(boundary) = boundary {
						self.result_map.insert(i, boundary);
					}
				}
				_ => {}
			}
		}
	}

	pub fn run(&mut self, code: &[Operator]) -> HashMap<usize, BoundaryType> {
		self.pending_stack.clear();

		self.run_tracking(code);

		std::mem::take(&mut self.result_map)
	}
}
