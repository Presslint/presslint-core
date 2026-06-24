# presslint-color Journal

## Current State

- Defines color policy data contracts for spot handling and overprint handling.
- Defines abstract transform requests over `presslint-core::ColorSpace`.
- Does not yet include ICC, DeviceLink, transform caching, or PDF write logic.

## Follow-Ups

- Keep color conversion as an action over inventory entries, not as parser
  orchestration.
