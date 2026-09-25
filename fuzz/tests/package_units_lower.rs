use plsql_core::{FileId, SymbolInterner};
use plsql_fuzz::package_source;
use plsql_ir::lower_top_level;
use plsql_parser::{ParseOptions, parse_with_backend};
use plsql_parser_antlr::Antlr4RustBackend;

const REGRESSION: &str = include_str!(
    "../corpus/package_units_lower/fuzz_regression_bb7e7fd29762e3cb7132f91e8d8822ee57e7c953.sql"
);

#[test]
fn fuzz_regression_bb7e7fd29762e3cb7132f91e8d8822ee57e7c953_recovered_duplicate_cursors() {
    let source = package_source(REGRESSION);
    let parsed = parse_with_backend(
        &source,
        FileId::new(0),
        &Antlr4RustBackend::new(),
        &ParseOptions::default(),
    );
    assert!(!parsed.is_clean() || parsed.recovered);

    let mut interner = SymbolInterner::new();
    let lowered = lower_top_level(&parsed.ast, &mut interner);
    let mut addressable_unit_count = 0;
    for declaration in lowered.declarations {
        let plsql_ir::Declaration::Package(package) = declaration else {
            continue;
        };
        let package_name = interner.resolve(package.common.name).unwrap_or("FUZZ_PKG");
        addressable_unit_count += package.units.addressable_units(package_name).len();
    }
    assert!(addressable_unit_count > 0);
}
