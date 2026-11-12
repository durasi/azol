// Run-time benchmark: the same computation as bench/loops.azl.

const size = 5000;
let total = 0;
let i = 0;
while (i < size) {
    let j = 0;
    while (j < size) {
        const product = i * j;
        if (product % 7 === 3) {
            total += product % 11;
        } else {
            total += 1;
        }
        j += 1;
    }
    i += 1;
}
console.log(total);
