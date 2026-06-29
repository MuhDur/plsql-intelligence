# syntax=docker/dockerfile:1
#
# plsql-mcp container image — the FULL PL/SQL Intelligence MCP server: live Oracle
# DB tools + guarded writes + offline PL/SQL intelligence (parse/analyze/depgraph/
# lineage/SAST). Live DB access routes through the pure-Rust thin stack
# (`oraclemcp-db` -> `oracledb`), so the image does not bundle Oracle Instant
# Client or native Oracle client libraries.
#
# Licensing: the binary + crates are Apache-2.0 OR MIT. Unofficial — not
# affiliated with Oracle Corporation.

# ---- builder: compile plsql-mcp (default features incl. live-db) ----
# plsql-parser-antlr's build.rs regenerates the Rust lexer/parser from the
# vendored antlr4-rust.jar, so it needs a JDK (Java 11+) on PATH — the GitHub
# ubuntu runners ship Java preinstalled, but oraclelinux:9 does not.
FROM oraclelinux:9 AS builder
RUN dnf -y install gcc java-17-openjdk-headless && dnf clean all && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain nightly-2026-05-11
ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /src
COPY . .
RUN cargo +nightly-2026-05-11 build --release -p plsql-mcp

# ---- runtime: plain Oracle Linux, no Instant Client layer ----
FROM oraclelinux:9
COPY --from=builder /src/target/release/plsql-mcp /usr/local/bin/plsql-mcp

# Required by the MCP registry to verify image ownership against server.json.
LABEL io.modelcontextprotocol.server.name="io.github.MuhDur/plsql-mcp"
LABEL org.opencontainers.image.title="PL/SQL Intelligence (plsql-mcp)"
LABEL org.opencontainers.image.description="Unofficial PL/SQL Intelligence MCP server — live Oracle DB tools + guarded writes + offline parse/analyze/depgraph/lineage/SAST. Superset of oraclemcp. Not affiliated with Oracle Corporation."
LABEL org.opencontainers.image.source="https://github.com/MuhDur/plsql-intelligence"
LABEL org.opencontainers.image.licenses="Apache-2.0 OR MIT"

ENTRYPOINT ["plsql-mcp"]
CMD ["serve"]
