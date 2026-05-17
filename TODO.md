# TODO

### Weaknesses (still rough)

- Version 0.1.1 — first public release today
- 35 clippy warnings — 22 auto-fixable, large_enum_variant, etc.
- 5 unwrap() in prod code (in sections.rs) — can panic on edge cases
- 1 expect() in prod code (render.rs) — can panic if kitty placement fails
- Zero integration tests — all 52 tests are unit tests
- No field history — 6 commits, all today, no iteration feedback
