//! Lexes arbitrary bytes and checks the lexer invariants.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| fernq::fuzz::lex(data));
