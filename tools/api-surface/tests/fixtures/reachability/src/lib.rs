mod private;
mod nested;
pub use private::{PublicThing, hidden_module as exposed_module};
pub use nested::{Alias as Renamed, *};

pub struct Root {
    pub field: u8,
    private_field: u8,
}

pub enum Choice {
    One { pub_field: u8 },
    Two(u8),
}

pub trait Behaviour {
    type Output;
    const READY: bool;
    fn run(&self);
}

impl Root {
    pub fn new() -> Self { Self { field: 0, private_field: 0 } }
}

impl Behaviour for Root {
    type Output = u8;
    const READY: bool = true;
    fn run(&self) {}
}
