# Agent workflow implementation

The implementation follows six independently verified stages: source analysis,
performance reports, expectation checks, source/artifact operations, compact Agent
responses, and bundled reference documentation. Runtime operations remain shared
by CLI and MCP; browser state remains complete.

## Source analysis

Text-command state ends at brace boundaries. Regression tests
cover header/markup, layout, paper, named music, inline Staff, explicit Voice,
unused variables, Unicode, BOM and CRLF. `cargo test -p ossia-score` verifies
source indexing, pitch prediction and byte-preserving tokenization.

Music analysis availability is separate from import conversion status. Unsupported
source must not be presented as a verified empty score.
