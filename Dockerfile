# syntax=docker/dockerfile:1
#
# plsql-mcp container image — the FULL PL/SQL Intelligence MCP server: live Oracle
# DB tools + guarded writes + offline PL/SQL intelligence (parse/analyze/depgraph/
# lineage/SAST). Oracle Instant Client is bundled so the live-DB tools work out of
# the box (plsql-mcp defaults to the `live-db` feature → ODPI-C via the `oracle`
# crate, which dlopen()s the client at runtime).
#
# Licensing: the binary + crates are Apache-2.0 OR MIT; the runtime layers come
# from Oracle's official Instant Client image (Oracle Free Use Terms), so this is
# a mixed-license artifact. Unofficial — not affiliated with Oracle Corporation.

# ---- builder: compile plsql-mcp (default features incl. live-db) ----
# ODPI-C is vendored + compiled by the `oracle` crate (needs gcc, not the client
# at build time). plsql-parser-antlr's build.rs regenerates the Rust lexer/parser
# from the vendored antlr4-rust.jar, so it needs a JDK (Java 11+) on PATH — the
# GitHub ubuntu runners ship Java preinstalled, but oraclelinux:9 does not.
FROM oraclelinux:9 AS builder
RUN dnf -y install gcc java-17-openjdk-headless && dnf clean all && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain nightly-2026-05-11
ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /src
COPY . .
RUN cargo build --release -p plsql-mcp

# ---- runtime: Oracle's official Instant Client image (public, FUTC) ----
FROM ghcr.io/oracle/oraclelinux9-instantclient:23
COPY --from=builder /src/target/release/plsql-mcp /usr/local/bin/plsql-mcp

# Required by the MCP registry to verify image ownership against server.json.
LABEL io.modelcontextprotocol.server.name="io.github.MuhDur/plsql-mcp"
LABEL org.opencontainers.image.title="PL/SQL Intelligence (plsql-mcp)"
LABEL org.opencontainers.image.description="Unofficial PL/SQL Intelligence MCP server — live Oracle DB tools + guarded writes + offline parse/analyze/depgraph/lineage/SAST. Superset of oraclemcp. Not affiliated with Oracle Corporation."
LABEL org.opencontainers.image.source="https://github.com/MuhDur/plsql-intelligence"
LABEL org.opencontainers.image.licenses="Apache-2.0 OR MIT"

ENTRYPOINT ["plsql-mcp"]
CMD ["serve"]
