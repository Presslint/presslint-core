# presslint-pdf Journal

## Current State

- Defines initial structural PDF access data contracts for indirect references
  and document info.
- Does not yet open PDF files, resolve object graphs, decode streams, or provide
  write seams.

## Follow-Ups

- Keep full PDF file parsing deferred until syntax, graphics-state, inventory,
  selectors, and planning/action slices have stable contracts.
