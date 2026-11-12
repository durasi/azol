// Run-time benchmark: the same computation as bench/append.azl.

let text = "";
let i = 0;
while (i < 3000000) {
    text = text + "0123456789";
    i += 1;
}
const lines = [];
let n = 0;
while (n < 300000) {
    const line = "line " + String(n);
    lines.push(line);
    n += 1;
}
const joined = lines.join("\n");
console.log(text.length + joined.length);
