use std::{
	io::{Result, Write},
	ops::Range,
};

use wasm_ast::node::{
	Block, Br, BrIf, BrTable, Call, CallIndirect, FuncData, If, LabelType, MemoryCopy, MemoryFill,
	MemoryGrow, ResultList, SetGlobal, SetLocal, SetTemporary, Statement, StoreAt, Terminator,
};
use wasmparser::ValType;

use crate::{
	analyzer::into_string::IntoName, backend::manager::write_separated, indentation, indented, line,
};

use super::{
	expression::Condition,
	manager::{Driver, Manager},
};

impl Driver for ResultList {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		write_separated(self.iter(), |t, w| t.write(mng, w), w)
	}
}

impl Driver for Br {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		if !self.align().is_aligned() {
			indentation!(mng, w)?;
			self.align().new_range().write(mng, w)?;
			write!(w, " = ")?;
			self.align().old_range().write(mng, w)?;
			writeln!(w)?;
		}

		if self.target() == 0 {
			if mng.label_list().last() == Some(&Some(LabelType::Backward)) {
				line!(mng, w, "continue")
			} else {
				line!(mng, w, "break")
			}
		} else {
			let level = mng.label_list().len() - 1 - self.target();

			line!(mng, w, "desired = {level}")?;
			line!(mng, w, "break")
		}
	}
}

fn to_ordered_table(list: &[Br], default: Br) -> Vec<Br> {
	let mut data: Vec<_> = list
		.iter()
		.copied()
		.chain(std::iter::once(default))
		.collect();

	data.sort_by_key(|v| v.target());
	data.dedup_by_key(|v| v.target());
	data
}

fn write_search_layer(
	range: Range<usize>,
	list: &[Br],
	mng: &mut Manager,
	w: &mut dyn Write,
) -> Result<()> {
	if range.len() == 1 {
		return list[range.start].write(mng, w);
	}

	let center = range.start + range.len() / 2;
	let br = list[center];

	if range.start != center {
		line!(mng, w, "if temp < {} then", br.target())?;
		mng.indent();
		write_search_layer(range.start..center, list, mng, w)?;
		mng.dedent();
		indented!(mng, w, "else")?;
	}

	if range.end != center + 1 {
		writeln!(w, "if temp > {} then", br.target())?;
		mng.indent();
		write_search_layer(center + 1..range.end, list, mng, w)?;
		mng.dedent();
		indented!(mng, w, "else")?;
	}

	writeln!(w)?;
	mng.indent();
	br.write(mng, w)?;
	mng.dedent();
	line!(mng, w, "end")
}

fn write_table_setup(table: &BrTable, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
	let id = mng.get_table_index(table);

	line!(mng, w, "if not br_map[{id}] then")?;
	mng.indent();
	line!(mng, w, "br_map[{id}] = (function()")?;
	mng.indent();
	indented!(mng, w, "return {{ [0] = ")?;

	table
		.data()
		.iter()
		.try_for_each(|v| write!(w, "{}, ", v.target()))?;

	writeln!(w, "}}")?;
	mng.dedent();
	line!(mng, w, "end)()")?;
	mng.dedent();
	line!(mng, w, "end")?;

	indented!(mng, w, "temp = br_map[{id}][")?;
	table.condition().write(mng, w)?;
	writeln!(w, "] or {}", table.default().target())
}

impl Driver for BrTable {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		if self.data().is_empty() {
			// Our condition should be pure so we probably don't need
			// to emit it in this case.
			return self.default().write(mng, w);
		}

		// `BrTable` is optimized by first mapping all indices to targets through
		// a Lua table; this reduces the size of the code generated as duplicate entries
		// don't need checking. Then, for speed, a binary search is done for the target
		// and the appropriate jump is performed.
		let list = to_ordered_table(self.data(), self.default());

		write_table_setup(self, mng, w)?;
		write_search_layer(0..list.len(), &list, mng, w)
	}
}

impl Driver for Terminator {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		match self {
			Self::Unreachable => line!(mng, w, r#"error("out of code bounds")"#),
			Self::Br(s) => s.write(mng, w),
			Self::BrTable(s) => s.write(mng, w),
		}
	}
}

fn write_br_parent(mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
	if !mng.has_branch() || mng.label_list().iter().all(Option::is_none) {
		return Ok(());
	}

	line!(mng, w, "if desired then")?;
	mng.indent();

	if let Some(last) = *mng.label_list().last().unwrap() {
		let level = mng.label_list().len() - 1;

		line!(mng, w, "if desired == {level} then")?;
		mng.indent();
		line!(mng, w, "desired = nil")?;

		if last == LabelType::Backward {
			line!(mng, w, "continue")?;
		}

		mng.dedent();
		line!(mng, w, "end")?;
	}

	line!(mng, w, "break")?;

	mng.dedent();
	line!(mng, w, "end")
}

impl Driver for Block {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		mng.push_label(self.label_type());

		line!(mng, w, "while true do")?;
		mng.indent();

		self.code().iter().try_for_each(|s| s.write(mng, w))?;

		match self.last() {
			Some(v) => v.write(mng, w)?,
			None => line!(mng, w, "break")?,
		}

		mng.dedent();
		line!(mng, w, "end")?;

		mng.pop_label();
		write_br_parent(mng, w)
	}
}

impl Driver for BrIf {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		indented!(mng, w, "if ")?;
		Condition(self.condition()).write(mng, w)?;
		writeln!(w, " then")?;
		mng.indent();
		self.target().write(mng, w)?;
		mng.dedent();
		line!(mng, w, "end")
	}
}

impl Driver for If {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		indented!(mng, w, "if ")?;
		Condition(self.condition()).write(mng, w)?;
		writeln!(w, " then")?;

		mng.indent();
		self.on_true().write(mng, w)?;
		mng.dedent();

		if let Some(v) = self.on_false() {
			line!(mng, w, "else")?;
			mng.indent();
			v.write(mng, w)?;
			mng.dedent();
		}

		line!(mng, w, "end")
	}
}

impl Driver for Call {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		if !self.result_list().is_empty() {
			self.result_list().write(mng, w)?;
			write!(w, " = ")?;
		}

		write!(w, "FUNC_LIST[{}](", self.function())?;
		self.param_list().write(mng, w)?;
		write!(w, ")")
	}
}

impl Driver for CallIndirect {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		if !self.result_list().is_empty() {
			self.result_list().write(mng, w)?;
			write!(w, " = ")?;
		}

		write!(w, "TABLE_LIST[{}].data[", self.table())?;
		self.index().write(mng, w)?;
		write!(w, "](")?;
		self.param_list().write(mng, w)?;
		write!(w, ")")
	}
}

impl Driver for SetTemporary {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		self.var().write(mng, w)?;
		write!(w, " = ")?;
		self.value().write(mng, w)
	}
}

impl Driver for SetLocal {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		self.var().write(mng, w)?;
		write!(w, " = ")?;
		self.value().write(mng, w)
	}
}

impl Driver for SetGlobal {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		write!(w, "GLOBAL_LIST[{}].value = ", self.var())?;
		self.value().write(mng, w)
	}
}

impl Driver for StoreAt {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		let name = self.store_type().into_name();
		let memory = self.memory();

		write!(w, "rt_store_{name}(memory_at_{memory}, ")?;

		self.pointer().write(mng, w)?;

		if self.offset() != 0 {
			write!(w, " + {}", self.offset())?;
		}

		write!(w, ", ")?;
		self.value().write(mng, w)?;
		write!(w, ")")
	}
}

impl Driver for MemoryGrow {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		let memory = self.memory();

		self.result().write(mng, w)?;
		write!(w, " = rt_allocator_grow(memory_at_{memory}, ")?;
		self.size().write(mng, w)?;
		write!(w, ")")
	}
}

impl Driver for MemoryCopy {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		let memory_1 = self.destination().memory();
		let memory_2 = self.source().memory();

		write!(w, "rt_store_copy(memory_at_{memory_1}, ")?;
		self.destination().pointer().write(mng, w)?;
		write!(w, ", memory_at_{memory_2}, ")?;
		self.source().pointer().write(mng, w)?;
		write!(w, ", ")?;
		self.size().write(mng, w)?;
		write!(w, ")")
	}
}

impl Driver for MemoryFill {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		let memory = self.destination().memory();

		write!(w, "rt_store_fill(memory_at_{memory}, ")?;
		self.destination().pointer().write(mng, w)?;
		write!(w, ", ")?;
		self.size().write(mng, w)?;
		write!(w, ", ")?;
		self.value().write(mng, w)?;
		write!(w, ")")
	}
}

fn write_stat(stat: &dyn Driver, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
	indentation!(mng, w)?;
	stat.write(mng, w)?;
	writeln!(w)
}

impl Driver for Statement {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		match self {
			Self::Block(s) => s.write(mng, w),
			Self::BrIf(s) => s.write(mng, w),
			Self::If(s) => s.write(mng, w),
			Self::Call(s) => write_stat(s, mng, w),
			Self::CallIndirect(s) => write_stat(s, mng, w),
			Self::SetTemporary(s) => write_stat(s, mng, w),
			Self::SetLocal(s) => write_stat(s, mng, w),
			Self::SetGlobal(s) => write_stat(s, mng, w),
			Self::StoreAt(s) => write_stat(s, mng, w),
			Self::MemoryGrow(s) => write_stat(s, mng, w),
			Self::MemoryCopy(s) => write_stat(s, mng, w),
			Self::MemoryFill(s) => write_stat(s, mng, w),
		}
	}
}

fn write_parameter_list(ast: &FuncData, w: &mut dyn Write) -> Result<()> {
	write!(w, "function(")?;
	write_separated(0..ast.num_param(), |i, w| write!(w, "loc_{i}"), w)?;
	writeln!(w, ")")
}

const fn type_to_zero(typ: ValType) -> &'static str {
	match typ {
		ValType::F32 | ValType::F64 => "0.0",
		ValType::I64 => "rt_i64_ZERO",
		_ => "0",
	}
}

fn write_variable_list(ast: &FuncData, mng: &Manager, w: &mut dyn Write) -> Result<()> {
	let mut locals = ast.local_data().iter().copied();
	let num_local = mng.num_local() - ast.num_param();

	for (i, typ) in locals.by_ref().enumerate().take(num_local) {
		let index = ast.num_param() + i;
		let zero = type_to_zero(typ);

		line!(mng, w, "local loc_{index} = {zero}")?;
	}

	if locals.len() != 0 {
		indented!(mng, w, "local loc_spill = {{ ")?;

		for typ in locals {
			let zero = type_to_zero(typ);

			write!(w, "{zero}, ")?;
		}

		writeln!(w, "}}")?;
	}

	let mut temporaries = 0..ast.num_stack();

	for i in temporaries.by_ref().take(mng.num_temp()) {
		line!(mng, w, "local reg_{i}")?;
	}

	if !temporaries.is_empty() {
		let len = temporaries.len();

		line!(mng, w, "local reg_spill = table.create({len})")?;
	}

	Ok(())
}

impl Driver for FuncData {
	fn write(&self, mng: &mut Manager, w: &mut dyn Write) -> Result<()> {
		mng.indent();

		write_parameter_list(self, w)?;
		write_variable_list(self, mng, w)?;

		if mng.has_branch() {
			line!(mng, w, "local desired")?;
		}

		if mng.has_table() {
			line!(mng, w, "local br_map = {{}}")?;
		}

		self.code().write(mng, w)?;

		if self.num_result() != 0 {
			indented!(mng, w, "return ")?;

			ResultList::new(0, self.num_result()).write(mng, w)?;

			writeln!(w)?;
		}

		mng.dedent();

		line!(mng, w, "end")
	}
}
