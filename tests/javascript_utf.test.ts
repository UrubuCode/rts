import test from "rts:test";

test.it("test utf-16 javascript");
const exemplo = "ação";
if(exemplo.length == 4) {
    test.pass();
}
else {
    test.fail()};
