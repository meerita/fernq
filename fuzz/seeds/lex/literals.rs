fn main() {
    let x: u32 = 1 + 2 * 3;
    let s = "a\n\u{e9}";
    let r = r#"say "hi""#;
    let b = b"\x00";
    let c = c"C";
    let ch = ('\'', 'é', b'\n');
    'outer: loop { break 'outer; }
    let f = 1.5e3_f64 + 2.;
}
