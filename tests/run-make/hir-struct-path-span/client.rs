#![feature(more_qualified_paths)]
extern crate qpath_macros;
extern crate qpath_support;

pub enum Value {
    #[deprecated(note = "qualified variant geometry")]
    Node {
        value: u8,
    },
    Other,
}
impl Value {
    pub fn ordinary_struct() -> Self {
        Self::Node { value: 1 }
    }
    pub fn qualified_struct() -> Self {
        <Self>::Node { value: 2 }
    }
    pub fn external_parts() -> Self {
        qpath_support::mixed_node!(Self, 3)
    }
    pub fn supplied_parts() -> Self {
        qpath_support::two_inputs!(Self, Node, 5)
    }
    pub fn procedural_parts() -> Self {
        qpath_macros::make_mixed!(Self)
    }
    pub fn matched_struct(self) -> u8 {
        match self {
            Self::Node { value } => value,
            _ => 0,
        }
    }
    pub fn extra_check() -> Self {
        Self::Node { value: 8 }
    }
}
include!("included.rs");
