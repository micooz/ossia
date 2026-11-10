# ossia-types

Shared serialized values for the local Rust core. JSON field names use camelCase; capability and error values use kebab-case. UUID identifiers are distinct Rust types and serialize as strings. `ContentHash` accepts only complete lowercase SHA-256 values.

`SourceSnapshot::new` validates UTF-8 and hashes the unchanged original bytes. Deserialization also checks the supplied hash. Bytes are private and exposed immutably; BOM, CRLF and trailing newlines survive unchanged. `SourceSpan` uses half-open UTF-8 byte ranges plus one-based Unicode scalar line/column endpoints; consumers validate spans against their source through `ossia-score::SourceMap`.

`Locator`, `LocatedRegion`, `LocationCapability` and `AppError` form the P01 analysis contracts. Protocol and schema versions start at 1. Later stages add their actual domain DTOs here and generate frontend declarations when wiring the SPA; P01 does not create runtime, version-store or frontend interfaces.
