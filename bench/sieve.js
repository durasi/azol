// Run-time benchmark: the same computation as bench/sieve.azl.

const limit = 20000000;
const composite = [];
let n = 0;
while (n <= limit) {
    composite.push(false);
    n += 1;
}
let count = 0;
let last = 0;
let i = 2;
while (i <= limit) {
    if (!composite[i]) {
        count += 1;
        last = i;
        let j = i * i;
        while (j <= limit) {
            composite[j] = true;
            j += i;
        }
    }
    i += 1;
}
console.log(count * 100000000 + last);
