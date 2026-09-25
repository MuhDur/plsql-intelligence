#![no_main]

use libfuzzer_sys::fuzz_target;
use plsql_core::{FileId, SymbolInterner};
use plsql_fuzz::package_source;
use plsql_ir::lower_top_level;
use plsql_parser::{ParseOptions, parse_with_backend};
use plsql_parser_antlr::Antlr4RustBackend;

const MAX_INPUT_BYTES: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }

    let Ok(body) = std::str::from_utf8(data) else {
        return;
    };
    if body
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\n' | '\r'))
    {
        return;
    }
    let source = package_source(body);

    let parsed = parse_with_backend(
        &source,
        FileId::new(0),
        &Antlr4RustBackend::new(),
        &ParseOptions::default(),
    );
    let mut interner = SymbolInterner::new();
    let lowered = lower_top_level(&parsed.ast, &mut interner);

    for declaration in lowered.declarations {
        if let plsql_ir::Declaration::Package(package) = declaration {
            let package_name = interner.resolve(package.common.name).unwrap_or("FUZZ_PKG");
            let _addressable_units = package.units.addressable_units(package_name);
        }
    }
});
