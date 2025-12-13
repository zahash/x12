use crate::qualifier::Qualifier;

pub struct ISA<'buf> {
    // ISA-01 -- ISA-04
    // These elements are hardly ever being used anymore.
    // They served authorization and security password purposes in the past.
    // They are now usually 00 or empty
    pub isa_01: &'buf [u8],
    pub isa_02: &'buf [u8],
    pub isa_03: &'buf [u8],
    pub isa_04: &'buf [u8],

    pub isa_05: Qualifier<'buf>, // sender qualifier
    pub isa_06: &'buf [u8],      // sender ID

    pub isa_07: Qualifier<'buf>, // receiver qualifier
    pub isa_08: &'buf [u8],      // receiver ID

    pub isa_09: IsaDate, // YYMMDD
    pub isa_10: IsaTime, // HHMM
}

#[derive(Debug, Clone, Copy)]
pub struct IsaDate {
    pub year: u8,  // 0 - 99
    pub month: u8, // 1 - 12
    pub day: u8,   // 1 - 31
}

#[derive(Debug, Clone, Copy)]
pub struct IsaTime {
    pub hour: u8,   // 0 - 23
    pub minute: u8, // 0 - 59
}

impl<'buf> ISA<'buf> {
    pub fn sender_qualifier(&self) -> Qualifier<'buf> {
        self.isa_05
    }

    pub fn sender_id(&self) -> &'buf [u8] {
        self.isa_06
    }

    pub fn receiver_qualifier(&self) -> Qualifier<'buf> {
        self.isa_07
    }

    pub fn receiver_id(&self) -> &'buf [u8] {
        self.isa_08
    }

    pub fn date(&self) -> IsaDate {
        self.isa_09
    }

    pub fn time(&self) -> IsaTime {
        self.isa_10
    }
}
