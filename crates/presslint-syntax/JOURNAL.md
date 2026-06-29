# presslint-syntax Journal

## Current State

- Crate root is a small public facade over focused syntax modules:
  `model`, `tokenizer`, `assembler`, and `serializer`.
- Implements source-preserving content-stream tokens with byte ranges.
- Tokenizer slice covers common lexical tokens, trivia, strings, delimiters,
  names, numbers, booleans, nulls, object/stream keywords, and operators.
- Serializer slice re-emits contiguous unmodified token streams
  byte-identically.
- Operator assembler groups top-level operands with operator records while
  preserving token references and source ranges.
- Assembly errors are structured for malformed operand/operator ordering.
- Criterion benchmark target `syntax` covers tokenize throughput in bytes,
  assemble throughput in token records, and unmodified serialization throughput
  over small and repeated synthetic public content streams.
- Tests are split into focused model, tokenizer, serializer, and assembler
  modules.

## Follow-Ups

- Keep syntax lexical/source-preserving; semantic interpretation belongs in
  inventory and later planning/action crates.
