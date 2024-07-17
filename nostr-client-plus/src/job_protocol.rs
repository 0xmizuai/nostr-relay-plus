#[repr(u16)]
pub enum Kind {
    NewJob = 6000,
    Alive,
    Assign,
    Result,
    Agg,
    Challenge,
    Resolution,
}

impl Kind {
    pub const NEW_JOB: u16 = Kind::NewJob as u16;
    pub const ALIVE: u16 = Kind::Alive as u16;
    pub const ASSIGN: u16 = Kind::Assign as u16;
    pub const RESULT: u16 = Kind::Result as u16;
    pub const AGG: u16 = Kind::Agg as u16;
    pub const CHALLENGE: u16 = Kind::Challenge as u16;
    pub const RESOLUTION: u16 = Kind::Resolution as u16;
}