use std::collections::{BTreeMap, BTreeSet};

use wasmparser::Operator;

use super::read_write_annotation::{ReadWriteLabel, Var};

#[derive(Default)]
pub struct DemandAnnotation {
	label_scratch: ReadWriteLabel,

	result_map: BTreeMap<usize, BTreeSet<Var>>,
}

impl DemandAnnotation {
	fn fill_indices(&mut self, label_map: &BTreeMap<usize, ReadWriteLabel>) {
		let iter = label_map.keys().map(|&i| (i, BTreeSet::new()));

		self.result_map.extend(iter);
	}

	fn patch_result(&mut self, key: usize) {
		self.result_map
			.insert(key, self.label_scratch.read_set().clone());
	}

	fn handle_block(&mut self, key: usize, label_map: &BTreeMap<usize, ReadWriteLabel>) {
		self.label_scratch.linear_merge(&label_map[&key]);

		self.patch_result(key);
	}

	fn handle_loop(&mut self, key: usize, label_map: &BTreeMap<usize, ReadWriteLabel>) {
		self.label_scratch.read_extend(label_map[&key].read_set());

		self.patch_result(key);
	}

	fn handle_if(&mut self, key: usize, label_map: &BTreeMap<usize, ReadWriteLabel>) {
		self.label_scratch.linear_merge(&label_map[&key]);

		self.patch_result(key);
	}

	fn handle_else(&mut self, key: usize) {
		let reset = self
			.result_map
			.iter()
			.find_map(|e| (*e.0 > key).then_some(e.1))
			.unwrap();

		self.label_scratch.clear();
		self.label_scratch.read_extend(reset);
	}

	fn run_tracking(&mut self, code: &[Operator], label_map: &BTreeMap<usize, ReadWriteLabel>) {
		for (i, inst) in code.iter().enumerate().rev() {
			match inst {
				Operator::Block { .. } => self.handle_block(i, label_map),
				Operator::Loop { .. } => self.handle_loop(i, label_map),
				Operator::If { .. } => self.handle_if(i, label_map),
				Operator::Else => self.handle_else(i),
				_ => {}
			}
		}
	}

	pub fn run(
		&mut self,
		code: &[Operator],
		label_map: &BTreeMap<usize, ReadWriteLabel>,
	) -> BTreeMap<usize, BTreeSet<Var>> {
		self.label_scratch.clear();

		self.fill_indices(label_map);
		self.run_tracking(code, label_map);
		self.handle_block(usize::MAX, label_map);

		std::mem::take(&mut self.result_map)
	}
}
