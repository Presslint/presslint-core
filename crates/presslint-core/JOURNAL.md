# presslint-core Journal

## Current State

- Defines shared page, object identity, byte range, provenance, content scope,
  color observation, object kind, and edit capability types.
- Provides the common data vocabulary used by inventory, selectors, actions,
  PDF access, syntax, and color crates.
- Performs no I/O.

## Follow-Ups

- Extend shared types only when a downstream slice needs a stable public
  contract.
