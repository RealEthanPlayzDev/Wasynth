use wasmparser::{BlockType, FunctionBody, MemArg, Operator, Result};

use crate::{
	module::{read_checked, read_checked_locals, TypeInfo},
	node::{
		BinOp, BinOpType, Block, Br, BrIf, BrTable, Call, CallIndirect, CmpOp, CmpOpType,
		Expression, FuncData, GetGlobal, If, LabelType, LoadAt, LoadType, Local, MemoryArgument,
		MemoryCopy, MemoryFill, MemoryGrow, MemorySize, Select, SetGlobal, SetLocal, Statement,
		StoreAt, StoreType, Terminator, UnOp, UnOpType, Value,
	},
	stack::{ReadGet, Stack},
};

#[derive(Clone, Copy)]
enum BlockVariant {
	Forward,
	Backward,
	If,
	Else,
}

enum BlockData {
	Forward { num_result: usize },
	Backward { num_param: usize },
	If { num_result: usize, ty: BlockType },
	Else { num_result: usize },
}

impl Default for BlockData {
	fn default() -> Self {
		Self::Forward { num_result: 0 }
	}
}

impl From<BlockData> for LabelType {
	fn from(data: BlockData) -> Self {
		match data {
			BlockData::Forward { .. } | BlockData::If { .. } | BlockData::Else { .. } => {
				Self::Forward
			}
			BlockData::Backward { .. } => Self::Backward,
		}
	}
}

#[derive(Default)]
struct StatList {
	stack: Stack,
	code: Vec<Statement>,
	last: Option<Box<Terminator>>,

	block_data: BlockData,
	has_reference: bool,
}

impl StatList {
	fn new() -> Self {
		Self::default()
	}

	fn leak_all(&mut self) {
		self.stack.leak_into(&mut self.code, |_| true);
	}

	fn leak_pre_call(&mut self) {
		self.stack.leak_into(&mut self.code, |node| {
			ReadGet::run(node, |_| false, |_| true, |_| true)
		});
	}

	fn leak_local_write(&mut self, id: usize) {
		self.stack.leak_into(&mut self.code, |node| {
			ReadGet::run(node, |var| var.var() == id, |_| false, |_| false)
		});
	}

	fn leak_global_write(&mut self, id: usize) {
		self.stack.leak_into(&mut self.code, |node| {
			ReadGet::run(node, |_| false, |var| var.var() == id, |_| false)
		});
	}

	fn leak_memory_write(&mut self, id: usize) {
		self.stack.leak_into(&mut self.code, |node| {
			ReadGet::run(node, |_| false, |_| false, |var| var.memory() == id)
		});
	}

	fn push_load(&mut self, load_type: LoadType, memarg: MemArg) {
		let memory = memarg.memory.try_into().unwrap();
		let offset = memarg.offset.try_into().unwrap();

		let data = Expression::LoadAt(LoadAt {
			load_type,
			memory,
			offset,
			pointer: self.stack.pop().into(),
		});

		self.stack.push(data);
	}

	fn add_store(&mut self, store_type: StoreType, memarg: MemArg) {
		let memory = memarg.memory.try_into().unwrap();
		let offset = memarg.offset.try_into().unwrap();

		let data = Statement::StoreAt(StoreAt {
			store_type,
			memory,
			offset,
			value: self.stack.pop().into(),
			pointer: self.stack.pop().into(),
		});

		self.leak_memory_write(memory);
		self.code.push(data);
	}

	fn push_constant<T: Into<Value>>(&mut self, value: T) {
		let value = Expression::Value(value.into());

		self.stack.push(value);
	}

	fn push_un_op(&mut self, op_type: UnOpType) {
		let data = Expression::UnOp(UnOp {
			op_type,
			rhs: self.stack.pop().into(),
		});

		self.stack.push(data);
	}

	fn push_bin_op(&mut self, op_type: BinOpType) {
		let data = Expression::BinOp(BinOp {
			op_type,
			rhs: self.stack.pop().into(),
			lhs: self.stack.pop().into(),
		});

		self.stack.push(data);
	}

	fn push_cmp_op(&mut self, op_type: CmpOpType) {
		let data = Expression::CmpOp(CmpOp {
			op_type,
			rhs: self.stack.pop().into(),
			lhs: self.stack.pop().into(),
		});

		self.stack.push(data);
	}

	// Eqz is the only unary comparison so it's "emulated"
	// using a constant operand
	fn try_add_equal_zero(&mut self, op: &Operator) -> bool {
		match op {
			Operator::I32Eqz => {
				self.push_constant(0_i32);
				self.push_cmp_op(CmpOpType::Eq_I32);

				true
			}
			Operator::I64Eqz => {
				self.push_constant(0_i64);
				self.push_cmp_op(CmpOpType::Eq_I64);

				true
			}
			_ => false,
		}
	}

	// Try to generate a simple operation
	fn try_add_operation(&mut self, op: &Operator) -> bool {
		if let Ok(op_type) = UnOpType::try_from(op) {
			self.push_un_op(op_type);

			true
		} else if let Ok(op_type) = BinOpType::try_from(op) {
			self.push_bin_op(op_type);

			true
		} else if let Ok(op_type) = CmpOpType::try_from(op) {
			self.push_cmp_op(op_type);

			true
		} else {
			self.try_add_equal_zero(op)
		}
	}

	fn set_terminator(&mut self, term: Terminator) {
		self.leak_all();
		self.last = Some(term.into());
	}
}

impl From<StatList> for Block {
	fn from(stat: StatList) -> Self {
		let label_type = stat.has_reference.then(|| stat.block_data.into());

		Self {
			label_type,
			code: stat.code,
			last: stat.last,
		}
	}
}

pub struct Factory<'a> {
	type_info: &'a TypeInfo<'a>,

	pending: Vec<StatList>,
	target: StatList,

	nested_unreachable: usize,
}

impl<'a> Factory<'a> {
	#[must_use]
	pub fn from_type_info(type_info: &'a TypeInfo<'a>) -> Self {
		Self {
			type_info,
			pending: Vec::new(),
			target: StatList::new(),
			nested_unreachable: 0,
		}
	}

	#[must_use]
	pub fn create_anonymous(&mut self, list: &[Operator]) -> FuncData {
		let data = self.build_stat_list(list, 1);

		FuncData {
			local_data: Vec::new(),
			num_result: 1,
			num_param: 0,
			num_stack: data.stack.capacity,
			code: data.into(),
		}
	}

	/// # Errors
	///
	/// Returns an error if the function is malformed.
	pub fn create_indexed(&mut self, index: usize, func: &FunctionBody) -> Result<FuncData> {
		let code = read_checked(func.get_operators_reader()?)?;
		let local_data = read_checked_locals(func.get_locals_reader()?)?;

		let (num_param, num_result) = self.type_info.by_func_index(index);
		let data = self.build_stat_list(&code, num_result);

		Ok(FuncData {
			local_data,
			num_result,
			num_param,
			num_stack: data.stack.capacity,
			code: data.into(),
		})
	}

	fn start_block(&mut self, ty: BlockType, variant: BlockVariant) {
		let (num_param, num_result) = self.type_info.by_block_type(ty);
		let mut old = std::mem::take(&mut self.target);

		old.leak_all();

		self.target.block_data = match variant {
			BlockVariant::Forward => BlockData::Forward { num_result },
			BlockVariant::Backward => BlockData::Backward { num_param },
			BlockVariant::If => BlockData::If { num_result, ty },
			BlockVariant::Else => {
				old.stack.pop_len(num_result).for_each(drop);
				old.stack.push_temporaries(num_param);

				BlockData::Else { num_result }
			}
		};

		self.target.stack = old.stack.split_last(num_param, num_result);

		old.stack.push_temporaries(num_result);

		self.pending.push(old);
	}

	fn start_else(&mut self) {
		let BlockData::If { ty, .. } = self.target.block_data else {
			unreachable!()
		};

		self.target.leak_all();
		self.end_block();
		self.start_block(ty, BlockVariant::Else);
	}

	fn end_block(&mut self) {
		let old = self.pending.pop().unwrap();
		let now = std::mem::replace(&mut self.target, old);

		self.target.stack.capacity = now.stack.capacity;

		let stat = match now.block_data {
			BlockData::Forward { .. } | BlockData::Backward { .. } => Statement::Block(now.into()),
			BlockData::If { .. } => Statement::If(If {
				condition: self.target.stack.pop().into(),
				on_true: Box::new(now.into()),
				on_false: None,
			}),
			BlockData::Else { .. } => {
				let Statement::If(last) = self.target.code.last_mut().unwrap() else {
					unreachable!()
				};

				last.on_false = Some(Box::new(now.into()));

				return;
			}
		};

		self.target.code.push(stat);
	}

	fn get_relative_block(&mut self, index: usize) -> &mut StatList {
		if index == 0 {
			&mut self.target
		} else {
			let index = self.pending.len() - index;

			&mut self.pending[index]
		}
	}

	fn get_br_terminator(&mut self, target: usize) -> Br {
		let block = self.get_relative_block(target);
		let previous = block.stack.previous;
		let result = match block.block_data {
			BlockData::Forward { num_result }
			| BlockData::If { num_result, .. }
			| BlockData::Else { num_result } => num_result,
			BlockData::Backward { num_param } => num_param,
		};

		block.has_reference = true;

		let align = self.target.stack.get_br_alignment(previous, result);

		Br { target, align }
	}

	fn add_call(&mut self, function: usize) {
		let (num_param, num_result) = self.type_info.by_func_index(function);
		let param_list = self.target.stack.pop_len(num_param).collect();

		self.target.leak_pre_call();

		let result_list = self.target.stack.push_temporaries(num_result);

		let data = Statement::Call(Call {
			function,
			param_list,
			result_list,
		});

		self.target.code.push(data);
	}

	fn add_call_indirect(&mut self, ty: usize, table: usize) {
		let (num_param, num_result) = self.type_info.by_type_index(ty);
		let index = self.target.stack.pop().into();
		let param_list = self.target.stack.pop_len(num_param).collect();

		self.target.leak_pre_call();

		let result_list = self.target.stack.push_temporaries(num_result);

		let data = Statement::CallIndirect(CallIndirect {
			table,
			index,
			param_list,
			result_list,
		});

		self.target.code.push(data);
	}

	#[cold]
	fn drop_unreachable(&mut self, op: &Operator) {
		match op {
			Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => {
				self.nested_unreachable += 1;
			}
			Operator::Else if self.nested_unreachable == 1 => {
				self.nested_unreachable -= 1;

				self.start_else();
			}
			Operator::End if self.nested_unreachable == 1 => {
				self.nested_unreachable -= 1;

				self.end_block();
			}
			Operator::End => {
				self.nested_unreachable -= 1;
			}
			_ => {}
		}
	}

	#[allow(clippy::too_many_lines)]
	fn add_instruction(&mut self, op: &Operator) {
		if self.target.try_add_operation(op) {
			return;
		}

		match *op {
			Operator::Unreachable => {
				self.nested_unreachable += 1;

				self.target.set_terminator(Terminator::Unreachable);
			}
			Operator::Nop => {}
			Operator::Block { blockty } => {
				self.start_block(blockty, BlockVariant::Forward);
			}
			Operator::Loop { blockty } => {
				self.start_block(blockty, BlockVariant::Backward);
			}
			Operator::If { blockty } => {
				let cond = self.target.stack.pop();

				self.start_block(blockty, BlockVariant::If);
				self.pending.last_mut().unwrap().stack.push(cond);
			}
			Operator::Else => {
				self.start_else();
			}
			Operator::End => {
				self.target.leak_all();
				self.end_block();
			}
			Operator::Br { relative_depth } => {
				let target = relative_depth.try_into().unwrap();
				let term = Terminator::Br(self.get_br_terminator(target));

				self.target.set_terminator(term);
				self.nested_unreachable += 1;
			}
			Operator::BrIf { relative_depth } => {
				let target = relative_depth.try_into().unwrap();
				let data = Statement::BrIf(BrIf {
					condition: self.target.stack.pop().into(),
					target: self.get_br_terminator(target),
				});

				self.target.leak_all();
				self.target.code.push(data);
			}
			Operator::BrTable { ref targets } => {
				let condition = self.target.stack.pop().into();
				let data = targets
					.targets()
					.map(Result::unwrap)
					.map(|v| self.get_br_terminator(v.try_into().unwrap()))
					.collect();

				let default = self.get_br_terminator(targets.default().try_into().unwrap());

				let term = Terminator::BrTable(BrTable {
					condition,
					data,
					default,
				});

				self.target.set_terminator(term);
				self.nested_unreachable += 1;
			}
			Operator::Return => {
				let target = self.pending.len();
				let term = Terminator::Br(self.get_br_terminator(target));

				self.target.set_terminator(term);
				self.nested_unreachable += 1;
			}
			Operator::Call { function_index } => {
				let index = function_index.try_into().unwrap();

				self.add_call(index);
			}
			Operator::CallIndirect {
				type_index,
				table_index,
				..
			} => {
				let type_index = type_index.try_into().unwrap();
				let table_index = table_index.try_into().unwrap();

				self.add_call_indirect(type_index, table_index);
			}
			Operator::Drop => {
				self.target.stack.pop();
			}
			Operator::Select => {
				let data = Expression::Select(Select {
					condition: self.target.stack.pop().into(),
					on_false: self.target.stack.pop().into(),
					on_true: self.target.stack.pop().into(),
				});

				self.target.stack.push(data);
			}
			Operator::LocalGet { local_index } => {
				let var = local_index.try_into().unwrap();
				let data = Expression::GetLocal(Local { var });

				self.target.stack.push(data);
			}
			Operator::LocalSet { local_index } => {
				let var = local_index.try_into().unwrap();
				let data = Statement::SetLocal(SetLocal {
					var: Local { var },
					value: self.target.stack.pop().into(),
				});

				self.target.leak_local_write(var);
				self.target.code.push(data);
			}
			Operator::LocalTee { local_index } => {
				let var = local_index.try_into().unwrap();
				let get = Expression::GetLocal(Local { var });
				let set = Statement::SetLocal(SetLocal {
					var: Local { var },
					value: self.target.stack.pop().into(),
				});

				self.target.leak_local_write(var);
				self.target.stack.push(get);
				self.target.code.push(set);
			}
			Operator::GlobalGet { global_index } => {
				let var = global_index.try_into().unwrap();
				let data = Expression::GetGlobal(GetGlobal { var });

				self.target.stack.push(data);
			}
			Operator::GlobalSet { global_index } => {
				let var = global_index.try_into().unwrap();
				let data = Statement::SetGlobal(SetGlobal {
					var,
					value: self.target.stack.pop().into(),
				});

				self.target.leak_global_write(var);
				self.target.code.push(data);
			}
			Operator::I32Load { memarg } => self.target.push_load(LoadType::I32, memarg),
			Operator::I64Load { memarg } => self.target.push_load(LoadType::I64, memarg),
			Operator::F32Load { memarg } => self.target.push_load(LoadType::F32, memarg),
			Operator::F64Load { memarg } => self.target.push_load(LoadType::F64, memarg),
			Operator::I32Load8S { memarg } => self.target.push_load(LoadType::I32_I8, memarg),
			Operator::I32Load8U { memarg } => self.target.push_load(LoadType::I32_U8, memarg),
			Operator::I32Load16S { memarg } => self.target.push_load(LoadType::I32_I16, memarg),
			Operator::I32Load16U { memarg } => self.target.push_load(LoadType::I32_U16, memarg),
			Operator::I64Load8S { memarg } => self.target.push_load(LoadType::I64_I8, memarg),
			Operator::I64Load8U { memarg } => self.target.push_load(LoadType::I64_U8, memarg),
			Operator::I64Load16S { memarg } => self.target.push_load(LoadType::I64_I16, memarg),
			Operator::I64Load16U { memarg } => self.target.push_load(LoadType::I64_U16, memarg),
			Operator::I64Load32S { memarg } => self.target.push_load(LoadType::I64_I32, memarg),
			Operator::I64Load32U { memarg } => self.target.push_load(LoadType::I64_U32, memarg),
			Operator::I32Store { memarg } => self.target.add_store(StoreType::I32, memarg),
			Operator::I64Store { memarg } => self.target.add_store(StoreType::I64, memarg),
			Operator::F32Store { memarg } => self.target.add_store(StoreType::F32, memarg),
			Operator::F64Store { memarg } => self.target.add_store(StoreType::F64, memarg),
			Operator::I32Store8 { memarg } => self.target.add_store(StoreType::I32_N8, memarg),
			Operator::I32Store16 { memarg } => self.target.add_store(StoreType::I32_N16, memarg),
			Operator::I64Store8 { memarg } => self.target.add_store(StoreType::I64_N8, memarg),
			Operator::I64Store16 { memarg } => self.target.add_store(StoreType::I64_N16, memarg),
			Operator::I64Store32 { memarg } => self.target.add_store(StoreType::I64_N32, memarg),
			Operator::MemorySize { mem, .. } => {
				let memory = mem.try_into().unwrap();
				let data = Expression::MemorySize(MemorySize { memory });

				self.target.stack.push(data);
			}
			Operator::MemoryGrow { mem, .. } => {
				let size = self.target.stack.pop().into();
				let result = self.target.stack.push_temporary();
				let memory = mem.try_into().unwrap();

				let data = Statement::MemoryGrow(MemoryGrow {
					memory,
					result,
					size,
				});

				self.target.leak_memory_write(memory);
				self.target.code.push(data);
			}
			Operator::MemoryCopy { dst_mem, src_mem } => {
				let size = self.target.stack.pop().into();

				let source = MemoryArgument {
					memory: src_mem.try_into().unwrap(),
					pointer: self.target.stack.pop().into(),
				};

				let destination = MemoryArgument {
					memory: dst_mem.try_into().unwrap(),
					pointer: self.target.stack.pop().into(),
				};

				self.target.leak_memory_write(source.memory);
				self.target.leak_memory_write(destination.memory);

				let data = Statement::MemoryCopy(MemoryCopy {
					destination,
					source,
					size,
				});

				self.target.code.push(data);
			}
			Operator::MemoryFill { mem } => {
				let size = self.target.stack.pop().into();
				let value = self.target.stack.pop().into();

				let destination = MemoryArgument {
					memory: mem.try_into().unwrap(),
					pointer: self.target.stack.pop().into(),
				};

				self.target.leak_memory_write(destination.memory);

				let data = Statement::MemoryFill(MemoryFill {
					destination,
					size,
					value,
				});

				self.target.code.push(data);
			}
			Operator::I32Const { value } => self.target.push_constant(value),
			Operator::I64Const { value } => self.target.push_constant(value),
			Operator::F32Const { value } => self.target.push_constant(value.bits()),
			Operator::F64Const { value } => self.target.push_constant(value.bits()),
			_ => panic!("Unsupported instruction: {op:?}"),
		}
	}

	fn build_stat_list(&mut self, list: &[Operator], num_result: usize) -> StatList {
		self.target.block_data = BlockData::Forward { num_result };
		self.nested_unreachable = 0;

		for op in list.iter().take(list.len() - 1) {
			if self.nested_unreachable == 0 {
				self.add_instruction(op);
			} else {
				self.drop_unreachable(op);
			}
		}

		if self.nested_unreachable == 0 {
			self.target.leak_all();
		}

		std::mem::take(&mut self.target)
	}
}
