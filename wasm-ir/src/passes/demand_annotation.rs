use std::collections::{HashMap, HashSet};

use wasmparser::Operator;

use super::{
	boundary_tracking::BoundaryType,
	read_write_annotation::{ReadWriteLabel, Var},
};

#[derive(Default)]
pub struct DemandAnnotation {
	label_scratch: ReadWriteLabel,
	label_stack: Vec<ReadWriteLabel>,

	result_map: HashMap<usize, HashSet<Var>>,
}

impl DemandAnnotation {
	fn fill_indices(&mut self, label_map: &HashMap<usize, ReadWriteLabel>) {
		let iter = label_map.keys().map(|&i| (i, HashSet::new()));

		self.result_map.extend(iter);
	}

	fn patch_result(&mut self, key: usize) {
		self.result_map
			.insert(key, self.label_scratch.read_set().clone());
	}

	fn handle_block(&mut self, key: usize, label_map: &HashMap<usize, ReadWriteLabel>) {
		self.label_scratch.linear_merge(&label_map[&key]);

		self.patch_result(key);
	}

	fn handle_loop(&mut self, key: usize, label_map: &HashMap<usize, ReadWriteLabel>) {
		self.label_scratch.read_extend(label_map[&key].read_set());

		self.patch_result(key);
	}

	fn handle_if(&mut self, key: usize, label_map: &HashMap<usize, ReadWriteLabel>) {
		self.label_scratch.linear_merge(&label_map[&key]);

		self.patch_result(key);
	}

	fn handle_else(&mut self) {
		let reset = self.label_stack.pop().unwrap();

		self.label_scratch = reset;
	}

	fn handle_end(
		&mut self,
		key: usize,
		boundary_map: &HashMap<usize, BoundaryType>,
		label_map: &HashMap<usize, ReadWriteLabel>,
	) {
		match boundary_map.get(&key) {
			Some(BoundaryType::Loop { start }) => {
				let data = label_map[start].read_set();

				self.label_scratch.read_extend(data);
			}
			Some(BoundaryType::Else) => {
				let clone = self.label_scratch.clone();

				self.label_stack.push(clone);
			}
			None => {}
		}
	}

	fn run_tracking(
		&mut self,
		code: &[Operator],
		boundary_map: &HashMap<usize, BoundaryType>,
		label_map: &HashMap<usize, ReadWriteLabel>,
	) {
		for (i, inst) in code.iter().enumerate().rev() {
			match inst {
				Operator::Block { .. } => self.handle_block(i, label_map),
				Operator::Loop { .. } => self.handle_loop(i, label_map),
				Operator::If { .. } => self.handle_if(i, label_map),
				Operator::Else => self.handle_else(),
				Operator::End => self.handle_end(i, boundary_map, label_map),
				_ => {}
			}
		}
	}

	pub fn run(
		&mut self,
		code: &[Operator],
		boundary_map: &HashMap<usize, BoundaryType>,
		label_map: &HashMap<usize, ReadWriteLabel>,
	) -> HashMap<usize, HashSet<Var>> {
		self.label_scratch.clear();
		self.label_stack.clear();

		self.fill_indices(label_map);
		self.run_tracking(code, boundary_map, label_map);
		self.handle_block(usize::MAX, label_map);

		std::mem::take(&mut self.result_map)
	}
}
