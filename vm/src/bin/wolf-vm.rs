//! wolf-vm - the wolf virtual machine
//!
//! Runs machine code generated from The Wolf Assembly Language

#![deny(unused_must_use)]

use std::path::PathBuf;
use std::fs::File;

use anyhow::Context;
use structopt::StructOpt;
use wolf_asm::executable::Executable;
use wolf_vm::{
    memory::Memory,
    write_memory::WriteMemory,
    registers::Registers,
    flags::Flags,
    machine::{Machine, ProgramStatus},
};

const MACHINE_MEMORY: usize = 4 * 1024; // 4 kb

/// The address where program execution should start
const START_ADDR: u64 = 0;

#[derive(Debug, StructOpt)]
#[structopt(name = "wolf-vm", about)]
struct VMOptions {
    /// The executable file generated by the wolf-asm assembler
    #[structopt(name = "input", parse(from_os_str))]
    executable_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let VMOptions {executable_path} = VMOptions::from_args();

    let executable_file = File::open(&executable_path)
        .with_context(|| format!("Failed to read executable: `{}`", executable_path.display()))?;
    let exec: Executable = bincode::deserialize_from(executable_file)
        .with_context(|| format!("Failed to deserialize executable: `{}`", executable_path.display()))?;

    let mut memory = Memory::new(MACHINE_MEMORY);
    // Write the executable at the starting address
    exec.write_into(&mut memory, START_ADDR)
        .context("Failed to load executable into memory")?;

    // Start with the stack pointer pointing just past the end of the stack
    let registers = Registers::new(MACHINE_MEMORY);
    let flags = Flags::default();

    let mut vm = Machine {
        program_counter: START_ADDR,
        memory,
        registers,
        flags,
    };
    vm.push_quit_addr()
        .expect("bug: should always be able to push quit address");

    loop {
        let pc = vm.program_counter;
        let status = vm.step()
            .with_context(|| format!("Failed to execute instruction at `0x{:x}`", pc))?;

        match status {
            ProgramStatus::Continue => {},
            ProgramStatus::Quit => break,
        }
    }

    Ok(())
}
