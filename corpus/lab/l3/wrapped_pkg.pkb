-- L3 realism: wrapped package body. DBMS_METADATA returns the wrapped
-- form; depgraph + parser must treat the body as opaque and surface an
-- UnknownReason::WrappedSource for any per-statement analysis.
--
-- Synthetic placeholder — a real wrapped unit starts with
-- `CREATE OR REPLACE PACKAGE BODY pkg_name wrapped a000000` followed
-- by base64-encoded gibberish. We use a marker that the corpus
-- ingestion path will treat the same way.
CREATE OR REPLACE PACKAGE BODY wrapped_pkg wrapped
a000000
b2
abcd
9 200 ...
/
