import { io } from "rts";

const c = "5";
io.print("c='" + c + "' code=" + c.charCodeAt(0).toString());
io.print("c >= '0' = " + (c >= "0").toString());
io.print("c <= '9' = " + (c <= "9").toString());
io.print("both = " + (c >= "0" && c <= "9").toString());

// alt: charCode
const code = c.charCodeAt(0);
io.print("code in 48..57 = " + (code >= 48 && code <= 57).toString());
