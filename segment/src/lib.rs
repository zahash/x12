#![no_std]

mod isa;
mod qualifier;

use isa::ISA;

pub enum Segment<'buf> {
    ISA(ISA<'buf>),
}
