// expected-kind: lexical-error
macro_rules! m {
    ($($t:tt)*) => {};
}
m!(##);
fn main() {}
