//! Page-object inventory model and graphics-state observations.

#![forbid(unsafe_code)]

mod digest;
mod inventory;
mod operands;
#[cfg(test)]
mod tests;
mod walker;

pub use inventory::{
    Inventory, InventoryEntry, build_form_inventory, build_image_inventory, build_text_inventory,
    build_vector_inventory, form_inventory_from_graphics_events,
    image_inventory_from_graphics_events, text_inventory_from_graphics_events,
    vector_inventory_from_graphics_events,
};
pub use walker::{
    GraphicsDeviceColor, GraphicsStateEvent, GraphicsStateEventKind, GraphicsStateSnapshot,
    GraphicsStateWalker, GraphicsWalkError, GraphicsWalkErrorKind, PathPaintKind,
    TextRenderingMode, TextShowOperator, walk_graphics_state,
};
