# presslint-pdf Journal

## Current State

- Defines initial structural PDF access data contracts for indirect references
  and document info.
- Models indirect-object edit ownership as proven single-use, shared, or
  unproven, with a pure helper that permits in-place mutation only for exactly
  one proven owning consumer.
- Does not yet open PDF files, resolve object graphs, decode streams, or provide
  write seams.

## Follow-Ups

- Keep full PDF file parsing deferred until syntax, graphics-state, inventory,
  selectors, and planning/action slices have stable contracts.
- Use the ownership decision model before future write planning mutates any
  indirect object that may be referenced by more than one consumer.
