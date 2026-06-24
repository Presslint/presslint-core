# presslint-selectors Journal

## Current State

- Defines serializable boolean selector expressions and leaf predicates.
- Current predicates cover object kind, observed color space, page index, and
  edit capability.
- Provides an in-memory matcher over `presslint_inventory::InventoryEntry`.

## Follow-Ups

- Add focused JSON serde-shape tests before adding CLI, MCP, or recipe
  consumers.
