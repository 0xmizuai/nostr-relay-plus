use std::error::Error;
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct Unrecoverable;

impl fmt::Display for Unrecoverable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Client Unrecoverable error")
    }
}

impl Error for Unrecoverable {}
