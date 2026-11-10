# Coding conventions

## Basic rules

- Fail fast with explicit errors; do not add fallback logic unless requested.
  Let errors propagate without handling them inside the business layer.
- Preserve existing changes. Do not revert staged files or check out, commit
  from, clean, or delete another session's worktree or logs.

## Structure and reuse

- Reuse existing utilities in the owning module. Put reusable general-purpose
  functions there when extraction is warranted.
- Call the owning module or service directly and keep simple one-use logic
  inline. Extract helpers or add abstractions to hide meaningful complexity,
  maintain a real boundary, or reuse logic. Avoid broad wrappers that only pass
  values through layers.
- Keep source files under 500 lines where practical by splitting them into
  focused modules. Unit test files are exempt.

## Types and documentation

- Use types from the owning library or module. Do not duplicate them locally or
  assert values as substitute types to work around TypeScript errors.
- Let TypeScript infer return types unless inference is incorrect or an explicit
  annotation is needed to preserve a public contract.
- Document externally exposed types, interfaces, functions, and classes. Add
  functional comments to key logic branches.

## Tests

- For a bug or issue, first reproduce it with a failing test, then verify the
  fix.
- Mirror the source structure: `crates/<crate>/tests/` for crate integration
  tests, `packages/ui/tests/` for `packages/ui/src/`. Each unit test file tests
  only its corresponding source file.
- Verify styles through component behavior and the toolchain. Do not test
  stylesheets or source files with regular-expression assertions.
