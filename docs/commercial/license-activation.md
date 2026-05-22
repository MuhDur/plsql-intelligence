<!--
SPDX-License-Identifier: Apache-2.0 OR MIT
-->

# License activation — not applicable

plsql-intelligence is fully open-source under `Apache-2.0 OR MIT`.
There is no license key, no activation step, and no commercial tier.

This file used to document a license-key activation flow for a
separate commercial MCP crate. That crate was merged into `plsql-mcp`
during the open-source consolidation, and the license gate was
removed. The eight former change-impact tools (`what_breaks`,
`classify_change`, `compare_oracle_deps`, `sarif_scan`,
`orphan_candidates`, `explain_lifecycle`, `release_gate`,
`recompile_plan`) now ship in `plsql-mcp` itself, in module
`change_tools`, and are available to everyone.

Nothing to activate: install `plsql-mcp`, run it, and the whole tool
surface is present. See [`../integrations/mcp-clients.md`](../integrations/mcp-clients.md)
for client setup.
