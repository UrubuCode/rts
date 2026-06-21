//! `URL.canParse` — exercises the static-overload-by-arity dispatch (the 1-arg and
//! 2-arg `canParse` overloads both resolve, picked by the caller's arg count).

use super::assert_stdout;

#[test]
fn can_parse_arity_overloads() {
    assert_stdout(r#"console.log(URL.canParse("https://x.com"));"#, "true\n");
    assert_stdout(r#"console.log(URL.canParse("/p", "https://x.com"));"#, "true\n");
    assert_stdout(r#"console.log(URL.canParse("not a url"));"#, "false\n");
}
