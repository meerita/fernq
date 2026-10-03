// expected-kind: lexical-error
macro_rules! m {
    ($($t:tt)*) => {};
}
m!(r#self);
fn main() {}
