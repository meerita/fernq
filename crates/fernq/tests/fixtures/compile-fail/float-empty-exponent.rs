// expected-kind: lexical-error
macro_rules! m {
    ($($t:tt)*) => {};
}
m!(1.0em);
fn main() {}
