pub mod element;
pub mod walker;

pub use element::{CodeElement, ElementType};
pub use walker::{index_single_file, walk_and_index};
