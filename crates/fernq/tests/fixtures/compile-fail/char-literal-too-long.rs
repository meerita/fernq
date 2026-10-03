// expected-kind: lexical-error
macro_rules! m {
    ($($t:tt)*) => {};
}
m!('ab');
fn main() {}
