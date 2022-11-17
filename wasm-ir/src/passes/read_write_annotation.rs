use std::collections::{HashMap, HashSet};

use wasmparser::Operator;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Var {
	Local(u32),
	Global(u32),
}

#[derive(Debug, Default, Clone)]
pub struct ReadWriteLabel {
	read_set: HashSet<Var>,
	write_set: HashSet<Var>,
}

impl ReadWriteLabel {
	pub fn read_set(&self) -> &HashSet<Var> {
		&self.read_set
	}

	pub fn clear(&mut self) {
		self.read_set.clear();
		self.write_set.clear();
	}

	pub fn read_extend(&mut self, other: &HashSet<Var>) {
		self.read_set.extend(other);
	}

	pub fn linear_merge(&mut self, other: &Self) {
		self.read_set.retain(|v| !other.write_set.contains(v));
		self.read_set.extend(&other.read_set);
		self.write_set.extend(&other.write_set);
	}

	fn branch_merge(&mut self, other: &Self) {
		self.read_set.extend(&other.read_set);
		self.write_set.retain(|v| other.write_set.contains(v));
	}
}

#[derive(Default)]
pub struct ReadWriteAnnotation {
	branch_stack: Vec<bool>,
	pending_stack: Vec<ReadWriteLabel>,

	result_map: HashMap<usize, ReadWriteLabel>,
	label_scratch: ReadWriteLabel,
}

impl ReadWriteAnnotation {
	fn handle_block(&mut self, key: usize) {
		let popped = self.pending_stack.pop().unwrap();

		self.branch_stack.pop().unwrap();
		self.result_map.insert(key, popped);
	}

	fn handle_if(&mut self, key: usize) {
		let mut popped = self.pending_stack.pop().unwrap();

		if self.branch_stack.pop().unwrap() {
			let other = self.pending_stack.pop().unwrap();

			popped.branch_merge(&other);
		}

		self.result_map.insert(key, popped);
	}

	fn handle_else(&mut self) {
		self.pending_stack.push(ReadWriteLabel::default());

		*self.branch_stack.last_mut().unwrap() = true;
	}

	fn handle_end(&mut self) {
		self.branch_stack.push(false);
		self.pending_stack.push(ReadWriteLabel::default());
	}

	fn handle_boundary(&mut self, key: usize, inst: &Operator) -> bool {
		match inst {
			Operator::Block { .. } | Operator::Loop { .. } => self.handle_block(key),
			Operator::If { .. } => self.handle_if(key),
			Operator::Else => self.handle_else(),
			Operator::End => self.handle_end(),
			_ => return false,
		}

		true
	}

	fn track_operation(&mut self, inst: &Operator) {
		let read_set = &mut self.label_scratch.read_set;
		let write_set = &mut self.label_scratch.write_set;

		match inst {
			Operator::LocalGet { local_index } => {
				read_set.insert(Var::Local(*local_index));
			}
			Operator::LocalSet { local_index } => {
				write_set.insert(Var::Local(*local_index));
			}
			Operator::LocalTee { local_index } => {
				read_set.insert(Var::Local(*local_index));
				write_set.insert(Var::Local(*local_index));
			}
			Operator::GlobalGet { global_index } => {
				read_set.insert(Var::Global(*global_index));
			}
			Operator::GlobalSet { global_index } => {
				write_set.insert(Var::Global(*global_index));
			}
			_ => {}
		}
	}

	fn add_label_data(&mut self, code: &[Operator]) {
		for (i, inst) in code.iter().enumerate().rev() {
			if self.handle_boundary(i, inst) {
				continue;
			}

			self.label_scratch.clear();

			self.track_operation(inst);

			self.pending_stack
				.last_mut()
				.unwrap()
				.linear_merge(&self.label_scratch);
		}
	}

	fn add_last_label(&mut self) {
		let last = self.pending_stack.pop().unwrap();

		self.result_map.insert(usize::MAX, last);
	}

	pub fn run(&mut self, code: &[Operator]) -> HashMap<usize, ReadWriteLabel> {
		self.branch_stack.clear();
		self.pending_stack.clear();
		self.label_scratch.clear();

		self.add_label_data(code);
		self.add_last_label();

		std::mem::take(&mut self.result_map)
	}
}
