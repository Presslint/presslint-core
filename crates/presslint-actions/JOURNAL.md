# presslint-actions Journal

## Current State

- Defines serializable recipe, recipe-step, action payload, and action-plan
  data contracts.
- Actions are requests only; they do not mutate documents directly.
- Depends on `presslint-selectors` for selector data and `presslint-core` for
  object identities.

## Follow-Ups

- Add the first no-op patch-plan/report slice after selector serde contracts are
  locked.
