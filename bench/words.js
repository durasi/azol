// Run-time benchmark: the same computation as bench/words.azl.

const names = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"];
let text = "";
let seed = 11;
let n = 0;
while (n < 400000) {
    seed = seed * 48271 % 2147483647;
    if (n > 0) {
        text = text + " ";
    }
    text = text + names[seed % 8];
    n += 1;
}
const words = text.split(" ");
let long = 0;
let withT = 0;
for (const word of words) {
    if (word.length > 4) {
        long += 1;
    }
    if (word.includes("t")) {
        withT += 1;
    }
}
const joined = words.join(",");
const parts = joined.split(",");
let starts = 0;
for (const part of parts) {
    if (part.startsWith("e")) {
        starts += 1;
    }
}
console.log(words.length + long * 3 + withT * 5 + starts * 7 + joined.length);
