# antlr/grammars-v4 PL/SQL test corpus

Vendored subset of the PL/SQL example files maintained by the
[antlr/grammars-v4](https://github.com/antlr/grammars-v4) project under
`sql/plsql/examples/`.

## License

Each `.g4` source in `grammars-v4` carries an explicit Apache-2.0 header (see
the file headers in the upstream repository). The repository's GitHub
metadata advertises the License as MIT. We treat the corpus files as
Apache-2.0, which is compatible with this project's dual Apache-2.0/MIT
license stack (see `LICENSE-APACHE`, `LICENSE-MIT`).

The bead `PLSQL-WS-013` references BSD-3 in its title; that is a bead-text
artifact and does not match the upstream license metadata. The corpus is
ingested under Apache-2.0 per the upstream headers.

## Files

The ingested subset focuses on small, single-statement DDL and SQL
expressions that exercise the parser's coverage. The intent is breadth, not
depth — adding more files is intentionally cheap (`PLSQL-CORPUS-CONTRIB-001`).

## Refresh

To refresh against upstream, re-run the curl invocation listed in
`corpus/manifest.toml` under the matching `[[file]]` entries' `source_url`
keys. Each ingested file's `fetched_on` date in the manifest reflects when
the snapshot was taken.
